//! In-process model-facing views over [`LearningBoundaryV1`].
//!
//! This module deliberately does not serialize anything. Content-addressed
//! public snapshots belong to the adjacent `public_information_snapshot`
//! adapter, outside the model-input batching hot path.
//!
//! This module removes artifact ids and schema labels from the inference view,
//! keeps variable candidate sets ragged, and decodes symbolic combat or run
//! selections without eagerly enumerating their combinatorial payloads.
//! It is compiled downstream from exact episode transitions so model-view
//! edits do not invalidate the optimized environment owner.

use std::collections::BTreeSet;
use std::fmt;

#[path = "learning_selection_inverse.rs"]
mod selection_inverse;

use crate::agent::information::action::{
    CombatSelectionDomainResolutionV1, CombatSelectionFamilyResolutionV1,
    PublicCombatAtomicActionV1, PublicCombatIndexedChoiceCandidateV1,
    PublicCombatIndexedChoiceReasonV1, PublicCombatSelectionDistinctByV1,
    PublicCombatSelectionDomainCandidateV1, PublicCombatSelectionFamilyV1,
};
use crate::agent::information::combat::HiddenInformationReasonV1;
use crate::agent::information::run::PublicCombatRunContextV1;
use crate::agent::information::state::{
    CombatLearningCardZonesV1, CombatLearningEncounterV1, CombatLearningMonsterStateV1,
    CombatLearningPlayerStateV1, CombatLearningPotionV1, CombatLearningTurnV1,
};
use crate::agent::task::TaskStrategicActionV1;
use crate::ai::planner_core::{
    CandidateRepresentationGap, CandidateSetCompleteness, PlannerAction, PlannerCardObservation,
    PlannerDecisionContext, PlannerDecisionSite, PlannerPotionSlotObservation,
    PlannerPublicHistory, PlannerPublicMap, PlannerRelicObservation, PlannerRunGoal,
    PlannerRunScalars,
};
use crate::content::cards::CardId;
use crate::runtime::combat::CardLineageId;
use crate::sim::combat_action_surface::{
    CombatIndexedChoiceInputEncodingV2, CombatSelectionInputEncodingV2,
    CombatSelectionPayloadLanguageV2, CombatSelectionReasonV2,
};
use crate::state::core::{ClientInput, PileType};
use crate::state::selection::{SelectionReason, SelectionResolution, SelectionScope};

use super::public_policy_candidate_admission::{
    admitted_strategic_candidate_ordinals_v1, public_combat_atomic_candidate_allowed_v1,
    PublicStrategicAdmissionKindV1,
};
use super::{
    LearningActionV1, LearningBoundaryV1, LearningCombatBoundaryV1, LearningStrategicBoundaryV1,
};

/// The semantic strategic state visible to a model.
///
/// Artifact identity, schema labels, and mechanics manifests remain outside
/// this view. They are dataset/runtime compatibility metadata, not features.
#[derive(Clone, Copy, Debug)]
pub struct LearningStrategicModelObservationV1<'a> {
    pub run_goal: PlannerRunGoal,
    pub decision_site: PlannerDecisionSite,
    pub run: &'a PlannerRunScalars,
    pub(crate) cards: &'a [PlannerCardObservation],
    pub relics: &'a [PlannerRelicObservation],
    potions: &'a [PlannerPotionSlotObservation],
    pub public_map: &'a PlannerPublicMap,
    pub context: &'a PlannerDecisionContext,
    pub public_history: &'a PlannerPublicHistory,
}

impl<'a> LearningStrategicModelObservationV1<'a> {
    pub fn cards(&self) -> impl ExactSizeIterator<Item = LearningStrategicCardV2<'a>> + '_ {
        self.cards
            .iter()
            .enumerate()
            .map(|(ordinal, card)| LearningStrategicCardV2 { ordinal, card })
    }

    /// Resolve a publicly named event effect to its current deck occurrence.
    /// Lineage is binding evidence only; callers receive an observation-local
    /// ordinal, never an identity feature. Missing, ambiguous or stale targets
    /// must not fall back to matching a card name or a deck position.
    pub fn card_effect_target_ordinal(
        &self,
        target: &PlannerCardObservation,
    ) -> Result<usize, LearningModelInputError> {
        let mut matches = self
            .cards
            .iter()
            .enumerate()
            .filter(|(_, card)| card.card_lineage == target.card_lineage);
        let Some((ordinal, card)) = matches.next() else {
            return Err(LearningModelInputError::StrategicCardTargetMissing);
        };
        if matches.next().is_some() {
            return Err(LearningModelInputError::StrategicCardTargetAmbiguous);
        }
        if card != target {
            return Err(LearningModelInputError::StrategicCardTargetMismatch);
        }
        Ok(ordinal)
    }

    pub fn potion_slots(
        &self,
    ) -> impl ExactSizeIterator<Item = LearningStrategicPotionSlotV1<'a>> + '_ {
        self.potions
            .iter()
            .map(|slot| LearningStrategicPotionSlotV1 { slot })
    }
}

/// One player-visible deck occurrence. The ordinal is local to the current
/// public deck ordering; the simulator card lineage is deliberately unavailable.
#[derive(Clone, Copy, Debug)]
pub struct LearningStrategicCardV2<'a> {
    ordinal: usize,
    card: &'a PlannerCardObservation,
}

impl LearningStrategicCardV2<'_> {
    pub fn ordinal(&self) -> usize {
        self.ordinal
    }

    pub fn card(&self) -> CardId {
        self.card.card
    }

    pub fn upgrades(&self) -> u8 {
        self.card.upgrades
    }

    pub fn misc_value(&self) -> i32 {
        self.card.misc_value
    }

    pub fn base_damage_override(&self) -> Option<i32> {
        self.card.base_damage_override
    }

    pub fn base_block_override(&self) -> Option<i32> {
        self.card.base_block_override
    }

    pub fn cost_modifier(&self) -> i8 {
        self.card.cost_modifier
    }
}

#[derive(Clone, Copy)]
pub struct LearningStrategicPotionSlotV1<'a> {
    slot: &'a PlannerPotionSlotObservation,
}

impl<'a> LearningStrategicPotionSlotV1<'a> {
    pub fn slot(&self) -> usize {
        self.slot.slot
    }

    pub fn potion(&self) -> Option<LearningStrategicPotionV1<'a>> {
        self.slot
            .potion
            .as_ref()
            .map(|potion| LearningStrategicPotionV1 { potion })
    }
}

#[derive(Clone, Copy)]
pub struct LearningStrategicPotionV1<'a> {
    potion: &'a crate::ai::planner_core::PlannerPotionObservation,
}

impl LearningStrategicPotionV1<'_> {
    pub fn potion(&self) -> crate::content::potions::PotionId {
        self.potion.potion
    }

    pub fn can_use(&self) -> bool {
        self.potion.can_use
    }

    pub fn can_discard(&self) -> bool {
        self.potion.can_discard
    }

    pub fn requires_target(&self) -> bool {
        self.potion.requires_target
    }
}

/// Public strategic candidate semantics. Card lineages, potion UUIDs, and opaque planner
/// candidate ids live only in the parallel resolution owned by the adapter.
/// Compatibility name for the strategic public action semantics now owned by
/// the deployable agent boundary.
pub type LearningStrategicActionSemanticsV2 = TaskStrategicActionV1;

#[derive(Clone, Copy)]
pub struct LearningStrategicActionV2<'a> {
    observation: LearningStrategicModelObservationV1<'a>,
    action: &'a PlannerAction,
}

impl fmt::Debug for LearningStrategicActionV2<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.semantics().fmt(formatter)
    }
}

impl LearningStrategicActionV2<'_> {
    pub fn semantics(&self) -> Result<LearningStrategicActionSemanticsV2, LearningModelInputError> {
        public_strategic_action_v2(self.observation, self.action)
    }
}

/// The semantic combat state visible to a model.
///
/// The learning environment has already established that this observation is
/// public and complete. Schema labels and completeness markers are therefore
/// validation metadata rather than model inputs.
#[derive(Clone, Copy, Debug)]
pub struct LearningCombatModelObservationV1<'a> {
    pub public_run_context: &'a PublicCombatRunContextV1,
    pub potions: &'a [Option<CombatLearningPotionV1>],
    pub hidden_reasons: &'a [HiddenInformationReasonV1],
    pub encounter: &'a CombatLearningEncounterV1,
    pub turn: &'a CombatLearningTurnV1,
    pub player: &'a CombatLearningPlayerStateV1,
    pub cards: &'a CombatLearningCardZonesV1,
    pub monsters: LearningCombatMonstersV1<'a>,
}

impl<'a> LearningCombatModelObservationV1<'a> {
    pub fn from_public_state(
        public_run_context: &'a PublicCombatRunContextV1,
        state: &'a crate::agent::information::state::PublicCombatStateV1,
    ) -> Self {
        Self {
            public_run_context,
            potions: &state.potions,
            hidden_reasons: &state.hidden_reasons,
            encounter: &state.encounter,
            turn: &state.turn,
            player: &state.player,
            cards: &state.cards,
            monsters: LearningCombatMonstersV1 {
                monsters: &state.monsters,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LearningModelObservationV1<'a> {
    Strategic(LearningStrategicModelObservationV1<'a>),
    Combat(LearningCombatModelObservationV1<'a>),
}

/// Candidate semantics supplied to an encoder.
///
/// The candidate ordinal is the only value returned by a policy. Opaque
/// candidate ids and exact runtime handles stay in the private resolution
/// table owned by [`LearningModelDecisionV1`].
#[derive(Clone, Copy, Debug)]
pub enum LearningModelCandidateSemanticsV1<'a> {
    Strategic {
        action: LearningStrategicActionV2<'a>,
    },
    CombatAtomic {
        action: LearningCombatAtomicActionV1<'a>,
    },
    CombatSelectionFamily {
        family: LearningCombatSelectionFamilyV1<'a>,
    },
}

/// Potion actions admitted to one model-facing combat candidate surface.
///
/// This policy never changes engine legality. An empty `RootSlots` set is the
/// explicit no-potion counterfactual. Non-empty root slots bind the exact
/// starting potion UUIDs, so consuming one does not authorize a generated
/// replacement that later occupies the same slot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum CombatLearningPotionPolicyV1 {
    #[default]
    All,
    RootSlots {
        requested_slots: Vec<usize>,
        potion_uuids: BTreeSet<u32>,
    },
}

impl CombatLearningPotionPolicyV1 {
    pub fn never() -> Self {
        Self::RootSlots {
            requested_slots: Vec::new(),
            potion_uuids: BTreeSet::new(),
        }
    }

    pub fn from_root_slots(
        root: &super::CombatLearningRootV1,
        requested_slots: impl IntoIterator<Item = usize>,
    ) -> Result<Self, String> {
        let requested_slots = requested_slots.into_iter().collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        let mut potion_uuids = BTreeSet::new();
        for slot in requested_slots.iter().copied() {
            if !seen.insert(slot) {
                return Err(format!(
                    "combat learning potion policy repeats root slot {slot}"
                ));
            }
            let Some(potion_uuid) = root.potion_uuids().get(slot) else {
                return Err(format!(
                    "combat learning potion policy root slot {slot} is out of range"
                ));
            };
            if let Some(potion_uuid) = potion_uuid {
                potion_uuids.insert(*potion_uuid);
            }
        }
        Ok(Self::RootSlots {
            requested_slots,
            potion_uuids,
        })
    }

    pub fn root_slots(&self) -> Option<&[usize]> {
        match self {
            Self::All => None,
            Self::RootSlots {
                requested_slots, ..
            } => Some(requested_slots),
        }
    }

    fn allows_input(&self, boundary: &LearningCombatBoundaryV1, input: &ClientInput) -> bool {
        let potion_slot = match input {
            ClientInput::UsePotion { potion_index, .. } => Some(*potion_index),
            ClientInput::DiscardPotion(slot) => Some(*slot),
            _ => None,
        };
        let Some(potion_slot) = potion_slot else {
            return true;
        };
        self.allows_potion_slot(boundary, potion_slot)
    }

    fn allows_potion_slot(&self, boundary: &LearningCombatBoundaryV1, potion_slot: usize) -> bool {
        match self {
            Self::All => true,
            Self::RootSlots { potion_uuids, .. } => boundary
                .private_resolution
                .potion_uuids_by_slot
                .get(potion_slot)
                .and_then(|potion_uuid| *potion_uuid)
                .is_some_and(|potion_uuid| potion_uuids.contains(&potion_uuid)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LearningCombatIndexedChoiceV1<'a> {
    pub input_encoding: CombatIndexedChoiceInputEncodingV2,
    pub reason: &'a PublicCombatIndexedChoiceReasonV1,
    pub candidate: &'a PublicCombatIndexedChoiceCandidateV1,
}

#[derive(Clone, Copy, Debug)]
pub enum LearningCombatAtomicActionV1<'a> {
    PlayCard {
        hand_index: usize,
        target_monster_index: Option<usize>,
    },
    UsePotion {
        potion_index: usize,
        target_monster_index: Option<usize>,
    },
    DiscardPotion {
        potion_index: usize,
    },
    EndTurn,
    SubmitIndexedChoice {
        choice_index: usize,
        indexed: LearningCombatIndexedChoiceV1<'a>,
    },
    Proceed,
    Cancel,
}

#[derive(Clone, Copy)]
pub struct LearningCombatMonstersV1<'a> {
    monsters: &'a [CombatLearningMonsterStateV1],
}

impl<'a> LearningCombatMonstersV1<'a> {
    pub fn len(&self) -> usize {
        self.monsters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.monsters.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<LearningCombatMonsterV1<'a>> {
        self.monsters
            .get(index)
            .map(|monster| LearningCombatMonsterV1 { monster })
    }

    pub(crate) fn as_slice(&self) -> &'a [CombatLearningMonsterStateV1] {
        self.monsters
    }
}

impl fmt::Debug for LearningCombatMonstersV1<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LearningCombatMonstersV1")
            .field("len", &self.len())
            .finish()
    }
}

#[derive(Clone, Copy)]
pub struct LearningCombatMonsterV1<'a> {
    monster: &'a CombatLearningMonsterStateV1,
}

impl<'a> LearningCombatMonsterV1<'a> {
    pub fn slot(&self) -> u8 {
        self.monster.slot
    }

    pub fn enemy(&self) -> crate::agent::information::state::CombatLearningEnemyIdentityV1 {
        self.monster.enemy
    }

    pub fn hp(&self) -> i32 {
        self.monster.hp
    }

    pub fn max_hp(&self) -> i32 {
        self.monster.max_hp
    }

    pub fn block(&self) -> i32 {
        self.monster.block
    }

    pub fn alive(&self) -> bool {
        self.monster.alive
    }

    pub fn escaped(&self) -> bool {
        self.monster.escaped
    }

    pub fn dying(&self) -> bool {
        self.monster.dying
    }

    pub fn half_dead(&self) -> bool {
        self.monster.half_dead
    }

    pub fn intent(&self) -> &'a crate::agent::information::state::CombatLearningIntentV1 {
        &self.monster.intent
    }

    pub fn executed_moves(
        &self,
    ) -> &'a crate::agent::information::state::CombatLearningMonsterMoveHistoryV1 {
        &self.monster.executed_moves
    }

    pub fn public_counters(
        &self,
    ) -> &'a [crate::agent::information::state::CombatLearningMonsterPublicCounterV1] {
        &self.monster.public_counters
    }

    pub fn powers(&self) -> &'a [crate::agent::information::state::CombatLearningPowerV1] {
        &self.monster.powers
    }
}

impl fmt::Debug for LearningCombatMonsterV1<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LearningCombatMonsterV1")
            .field("slot", &self.slot())
            .field("enemy", &self.enemy())
            .field("hp", &self.hp())
            .field("max_hp", &self.max_hp())
            .field("block", &self.block())
            .field("alive", &self.alive())
            .field("escaped", &self.escaped())
            .field("dying", &self.dying())
            .field("half_dead", &self.half_dead())
            .field("intent", &self.intent())
            .field("executed_moves", &self.executed_moves())
            .field("public_counters", &self.public_counters())
            .field("powers", &self.powers())
            .finish()
    }
}

#[derive(Clone, Copy)]
pub struct LearningCombatSelectionFamilyV1<'a> {
    family: &'a PublicCombatSelectionFamilyV1,
}

impl fmt::Debug for LearningCombatSelectionFamilyV1<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LearningCombatSelectionFamilyV1")
            .field("input_encoding", &self.input_encoding())
            .field("reason", &self.reason())
            .field("source_pile", &self.source_pile())
            .field("raw_domain_count", &self.raw_domain_count())
            .field("eligible_domain_count", &self.eligible_domain_count())
            .field("declared_min", &self.declared_min())
            .field("declared_max", &self.declared_max())
            .field("effective_max", &self.effective_max())
            .field("payload_language", &self.payload_language())
            .finish_non_exhaustive()
    }
}

impl<'a> LearningCombatSelectionFamilyV1<'a> {
    pub fn from_public(family: &'a PublicCombatSelectionFamilyV1) -> Self {
        Self { family }
    }

    pub fn input_encoding(&self) -> CombatSelectionInputEncodingV2 {
        self.family.input_encoding
    }

    pub fn reason(&self) -> &CombatSelectionReasonV2 {
        &self.family.reason
    }

    pub fn source_pile(&self) -> Option<PileType> {
        self.family.source_pile
    }

    pub fn raw_domain_count(&self) -> u64 {
        self.family.raw_domain_count
    }

    pub fn eligible_domain_count(&self) -> u64 {
        self.family.eligible_domain_count
    }

    pub fn max_distinct_selection_count(&self) -> u64 {
        self.family.max_distinct_selection_count
    }

    pub fn declared_min(&self) -> u64 {
        self.family.declared_min
    }

    pub fn declared_max(&self) -> u64 {
        self.family.declared_max
    }

    pub fn effective_max(&self) -> u64 {
        self.family.effective_max
    }

    pub fn payload_language(&self) -> CombatSelectionPayloadLanguageV2 {
        CombatSelectionPayloadLanguageV2::OrderedDistinctSequence(match self.family.distinct_by {
            PublicCombatSelectionDistinctByV1::CardOccurrence => {
                crate::sim::combat_action_surface::CombatSelectionDistinctByV2::CardLineage
            }
            PublicCombatSelectionDistinctByV1::ScryOccurrence => {
                crate::sim::combat_action_surface::CombatSelectionDistinctByV2::ScryIndexAndCardLineage
            }
        })
    }

    pub fn domain_count(&self) -> usize {
        self.family.domain.len()
    }

    pub fn domain(&self, index: usize) -> Option<LearningCombatSelectionDomainV1<'a>> {
        self.family
            .domain
            .get(index)
            .map(|domain| LearningCombatSelectionDomainV1 { domain })
    }
}

#[derive(Clone, Copy)]
pub struct LearningCombatSelectionDomainV1<'a> {
    domain: &'a PublicCombatSelectionDomainCandidateV1,
}

impl fmt::Debug for LearningCombatSelectionDomainV1<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("LearningCombatSelectionDomainV1")
            .field(&self.semantics())
            .finish()
    }
}

impl LearningCombatSelectionDomainV1<'_> {
    pub fn semantics(&self) -> LearningCombatSelectionDomainSemanticsV1 {
        match self.domain {
            PublicCombatSelectionDomainCandidateV1::Card {
                ordinal,
                card_id,
                upgrades,
                eligible,
                ..
            } => LearningCombatSelectionDomainSemanticsV1::Card {
                ordinal: *ordinal,
                card_id: *card_id,
                upgrades: *upgrades,
                eligible: *eligible,
            },
            PublicCombatSelectionDomainCandidateV1::Scry {
                index,
                card_id,
                currently_present,
                ..
            } => LearningCombatSelectionDomainSemanticsV1::Scry {
                index: *index,
                card_id: *card_id,
                currently_present: *currently_present,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearningCombatSelectionDomainSemanticsV1 {
    Card {
        ordinal: u64,
        card_id: Option<crate::content::cards::CardId>,
        upgrades: Option<u8>,
        eligible: bool,
    },
    Scry {
        index: u64,
        card_id: Option<crate::content::cards::CardId>,
        currently_present: bool,
    },
}

#[derive(Clone, Copy)]
pub struct LearningModelCandidateV1<'a> {
    pub semantics: LearningModelCandidateSemanticsV1<'a>,
    resolution: LearningCandidateResolutionV1<'a>,
}

impl fmt::Debug for LearningModelCandidateV1<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LearningModelCandidateV1")
            .field("semantics", &self.semantics)
            .finish()
    }
}

#[derive(Clone, Copy, Debug)]
enum LearningCandidateResolutionV1<'a> {
    StrategicCandidate {
        candidate_id: &'a str,
    },
    CombatAtomic {
        input: &'a ClientInput,
    },
    CombatSelectionFamily {
        public: &'a PublicCombatSelectionFamilyV1,
        private_resolution: &'a CombatSelectionFamilyResolutionV1,
    },
    RunSelectionFamily {
        planner_candidate_id: &'a str,
        scope: SelectionScope,
        reason: SelectionReason,
        min_choices: usize,
        max_choices: usize,
        selectable_card_lineages: &'a [CardLineageId],
    },
}

#[derive(Clone, Debug)]
pub struct LearningModelDecisionV1<'a> {
    pub observation: LearningModelObservationV1<'a>,
    pub candidates: Vec<LearningModelCandidateV1<'a>>,
}

/// The environment's legal choices and a task's reduced policy surface are
/// separate contracts. Neither surface changes simulator legality.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningCandidateSurfaceV1 {
    #[default]
    Policy,
    Legal,
}

impl<'a> LearningModelDecisionV1<'a> {
    pub fn from_boundary(
        boundary: &'a LearningBoundaryV1,
    ) -> Result<Self, LearningModelInputError> {
        Self::from_boundary_with_potion_policy(boundary, &CombatLearningPotionPolicyV1::All)
    }

    pub fn from_boundary_with_potion_policy(
        boundary: &'a LearningBoundaryV1,
        potion_policy: &CombatLearningPotionPolicyV1,
    ) -> Result<Self, LearningModelInputError> {
        Self::from_boundary_with_surface(
            boundary,
            potion_policy,
            LearningCandidateSurfaceV1::Policy,
        )
    }

    pub fn from_boundary_with_surface(
        boundary: &'a LearningBoundaryV1,
        potion_policy: &CombatLearningPotionPolicyV1,
        surface: LearningCandidateSurfaceV1,
    ) -> Result<Self, LearningModelInputError> {
        match boundary {
            LearningBoundaryV1::Strategic { boundary } => Self::from_strategic(boundary, surface),
            LearningBoundaryV1::Combat { boundary } => {
                Self::from_combat(boundary, potion_policy, surface)
            }
            LearningBoundaryV1::Terminal { .. } => Err(LearningModelInputError::TerminalBoundary),
            LearningBoundaryV1::Unsupported => Err(LearningModelInputError::UnsupportedBoundary),
        }
    }

    pub fn choose(
        &self,
        candidate_ordinal: usize,
    ) -> Result<LearningModelChoiceV1, LearningModelInputError> {
        let candidate = self.candidates.get(candidate_ordinal).ok_or(
            LearningModelInputError::CandidateOrdinalOutOfRange {
                candidate_ordinal,
                candidate_count: self.candidates.len(),
            },
        )?;
        Ok(match candidate.resolution {
            LearningCandidateResolutionV1::StrategicCandidate { candidate_id } => {
                LearningModelChoiceV1::Apply(LearningActionV1::StrategicCandidate {
                    candidate_id: candidate_id.to_string(),
                })
            }
            LearningCandidateResolutionV1::CombatAtomic { input } => {
                LearningModelChoiceV1::Apply(LearningActionV1::CombatInput {
                    input: input.clone(),
                })
            }
            LearningCandidateResolutionV1::CombatSelectionFamily {
                public,
                private_resolution,
            } => LearningModelChoiceV1::DecodeSelection(LearningSelectionDraftV1::from_combat(
                public.clone(),
                private_resolution.clone(),
            )),
            LearningCandidateResolutionV1::RunSelectionFamily {
                planner_candidate_id,
                scope,
                reason,
                min_choices,
                max_choices,
                selectable_card_lineages,
            } => LearningModelChoiceV1::DecodeSelection(LearningSelectionDraftV1::from_run(
                planner_candidate_id.to_string(),
                scope,
                reason,
                min_choices,
                max_choices,
                selectable_card_lineages.to_vec(),
                selectable_card_lineages
                    .iter()
                    .map(|lineage| match self.observation {
                        LearningModelObservationV1::Strategic(observation) => observation
                            .cards
                            .iter()
                            .position(|card| card.card_lineage == *lineage)
                            .ok_or(LearningModelInputError::StrategicCardTargetMissing),
                        LearningModelObservationV1::Combat(_) => {
                            Err(LearningModelInputError::StrategicCardTargetMissing)
                        }
                    })
                    .collect::<Result<_, _>>()?,
            )),
        })
    }

    /// Resolves one planner-owned strategic identity against this exact model surface.
    ///
    /// Candidate identities remain internal execution metadata and are never model
    /// features.  This join exists for bridge-side behavior labels after policy
    /// canonicalization has filtered or reordered the planner's complete legal set.
    /// Missing or ambiguous identities fail closed as unavailable.
    pub fn strategic_ordinal_for_planner_candidate_id(
        &self,
        planner_candidate_id: &str,
    ) -> Option<usize> {
        let mut matches = self
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(ordinal, candidate)| {
                let candidate_id = match candidate.resolution {
                    LearningCandidateResolutionV1::StrategicCandidate { candidate_id } => {
                        candidate_id
                    }
                    LearningCandidateResolutionV1::RunSelectionFamily {
                        planner_candidate_id,
                        ..
                    } => planner_candidate_id,
                    LearningCandidateResolutionV1::CombatAtomic { .. }
                    | LearningCandidateResolutionV1::CombatSelectionFamily { .. } => return None,
                };
                (candidate_id == planner_candidate_id).then_some(ordinal)
            });
        let ordinal = matches.next()?;
        matches.next().is_none().then_some(ordinal)
    }

    /// Resolves one exact atomic combat input against the model-facing surface.
    ///
    /// Exact-search laboratories use this typed join to bind counterfactual
    /// successor evidence to the same ragged candidate ordinal consumed by a
    /// learned policy. Inputs withheld by the learning policy prior, structured
    /// selection families, and ambiguous duplicates remain unavailable.
    pub fn combat_atomic_ordinal_for_input(&self, input: &ClientInput) -> Option<usize> {
        let mut matches = self
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(ordinal, candidate)| match candidate.resolution {
                LearningCandidateResolutionV1::CombatAtomic {
                    input: candidate_input,
                } if candidate_input == input => Some(ordinal),
                LearningCandidateResolutionV1::StrategicCandidate { .. }
                | LearningCandidateResolutionV1::CombatAtomic { .. }
                | LearningCandidateResolutionV1::CombatSelectionFamily { .. }
                | LearningCandidateResolutionV1::RunSelectionFamily { .. } => None,
            });
        let ordinal = matches.next()?;
        matches.next().is_none().then_some(ordinal)
    }

    pub fn from_strategic_boundary(
        boundary: &'a LearningStrategicBoundaryV1,
    ) -> Result<Self, LearningModelInputError> {
        Self::from_strategic(boundary, LearningCandidateSurfaceV1::Policy)
    }

    pub fn from_combat_boundary(
        boundary: &'a LearningCombatBoundaryV1,
    ) -> Result<Self, LearningModelInputError> {
        Self::from_combat(
            boundary,
            &CombatLearningPotionPolicyV1::All,
            LearningCandidateSurfaceV1::Policy,
        )
    }

    pub fn from_combat_boundary_with_potion_policy(
        boundary: &'a LearningCombatBoundaryV1,
        potion_policy: &CombatLearningPotionPolicyV1,
    ) -> Result<Self, LearningModelInputError> {
        Self::from_combat(boundary, potion_policy, LearningCandidateSurfaceV1::Policy)
    }

    fn from_strategic(
        boundary: &'a LearningStrategicBoundaryV1,
        surface: LearningCandidateSurfaceV1,
    ) -> Result<Self, LearningModelInputError> {
        if let CandidateSetCompleteness::Incomplete { gaps, .. } =
            &boundary.legal_candidates.completeness
        {
            return Err(LearningModelInputError::IncompleteStrategicCandidateSet {
                site: boundary.observation.decision_site,
                gaps: gaps.clone(),
            });
        }
        if boundary.legal_candidates.observation_id != boundary.observation.observation_id {
            return Err(LearningModelInputError::StrategicObservationIdentityMismatch);
        }
        if boundary.legal_candidates.site != boundary.observation.decision_site {
            return Err(LearningModelInputError::StrategicDecisionSiteMismatch);
        }
        if boundary
            .legal_candidates
            .candidates
            .iter()
            .any(|candidate| candidate.mechanics != boundary.observation.mechanics)
        {
            return Err(LearningModelInputError::StrategicMechanicsMismatch);
        }

        let observation = &boundary.observation;
        let model_observation = LearningStrategicModelObservationV1 {
            run_goal: observation.run_goal,
            decision_site: observation.decision_site,
            run: &observation.run,
            cards: &observation.cards,
            relics: &observation.relics,
            potions: &observation.potions,
            public_map: &observation.public_map,
            context: &observation.context,
            public_history: &observation.public_history,
        };
        let admitted = match surface {
            LearningCandidateSurfaceV1::Policy => strategic_learning_policy_candidates(boundary),
            LearningCandidateSurfaceV1::Legal => {
                boundary.legal_candidates.candidates.iter().collect()
            }
        };
        let candidates = admitted
            .into_iter()
            .map(|candidate| {
                let resolution = match &candidate.action {
                    PlannerAction::BeginRunCardSelection {
                        scope,
                        reason,
                        min_choices,
                        max_choices,
                        selectable_card_lineages,
                    } => LearningCandidateResolutionV1::RunSelectionFamily {
                        planner_candidate_id: &candidate.candidate_id,
                        scope: *scope,
                        reason: *reason,
                        min_choices: *min_choices,
                        max_choices: *max_choices,
                        selectable_card_lineages,
                    },
                    _ => LearningCandidateResolutionV1::StrategicCandidate {
                        candidate_id: &candidate.candidate_id,
                    },
                };
                LearningModelCandidateV1 {
                    semantics: LearningModelCandidateSemanticsV1::Strategic {
                        action: LearningStrategicActionV2 {
                            observation: model_observation,
                            action: &candidate.action,
                        },
                    },
                    resolution,
                }
            })
            .collect::<Vec<_>>();
        ensure_nonempty(candidates.len())?;

        Ok(Self {
            observation: LearningModelObservationV1::Strategic(model_observation),
            candidates,
        })
    }

    fn from_combat(
        boundary: &'a LearningCombatBoundaryV1,
        potion_policy: &CombatLearningPotionPolicyV1,
        surface: LearningCandidateSurfaceV1,
    ) -> Result<Self, LearningModelInputError> {
        validate_public_private_action_alignment(boundary)?;

        let observation = &boundary.observation;
        let admitted_atomic_actions = boundary
            .public_actions
            .atomic_actions
            .iter()
            .zip(&boundary.private_resolution.atomic_inputs)
            .filter_map(|(action, input)| {
                potion_policy
                    .allows_input(boundary, input)
                    .then_some(action)
            })
            .collect::<Vec<_>>();
        let mut candidates = Vec::with_capacity(
            boundary.public_actions.atomic_actions.len()
                + boundary.public_actions.selection_families.len(),
        );
        for (action, input) in boundary
            .public_actions
            .atomic_actions
            .iter()
            .zip(&boundary.private_resolution.atomic_inputs)
        {
            if !potion_policy.allows_input(boundary, input) {
                continue;
            }
            if surface == LearningCandidateSurfaceV1::Policy
                && !public_combat_atomic_candidate_allowed_v1(
                    &boundary.observation,
                    action,
                    &admitted_atomic_actions,
                )
            {
                continue;
            }
            candidates.push(LearningModelCandidateV1 {
                semantics: LearningModelCandidateSemanticsV1::CombatAtomic {
                    action: learning_atomic_semantics(boundary, action)?,
                },
                resolution: LearningCandidateResolutionV1::CombatAtomic { input },
            });
        }
        for (public, private_resolution) in boundary
            .public_actions
            .selection_families
            .iter()
            .zip(&boundary.private_resolution.selection_families)
        {
            candidates.push(LearningModelCandidateV1 {
                semantics: LearningModelCandidateSemanticsV1::CombatSelectionFamily {
                    family: LearningCombatSelectionFamilyV1 { family: public },
                },
                resolution: LearningCandidateResolutionV1::CombatSelectionFamily {
                    public,
                    private_resolution,
                },
            });
        }
        ensure_nonempty(candidates.len())?;

        Ok(Self {
            observation: LearningModelObservationV1::Combat(LearningCombatModelObservationV1 {
                public_run_context: &boundary.public_run_context,
                potions: &observation.potions,
                hidden_reasons: &observation.hidden_reasons,
                encounter: &observation.encounter,
                turn: &observation.turn,
                player: &observation.player,
                cards: &observation.cards,
                monsters: LearningCombatMonstersV1 {
                    monsters: &observation.monsters,
                },
            }),
            candidates,
        })
    }
}

fn public_strategic_action_v2(
    observation: LearningStrategicModelObservationV1<'_>,
    action: &PlannerAction,
) -> Result<LearningStrategicActionSemanticsV2, LearningModelInputError> {
    TaskStrategicActionV1::from_planner_action_v1(observation.cards, action)
        .map_err(|_| LearningModelInputError::StrategicCardTargetMissing)
}

/// Removes mechanically dominated reward clicks from the learned policy surface.
///
/// The full planner candidate set remains the source of legal runtime actions.
/// A learner is only asked to choose after deterministic free resources and
/// potion claims have been resolved. Reversible card reveals remain choices,
/// so skipping a card can still be followed by navigation. This preserves
/// choices such as card versus skip, a full potion inventory, and route
/// commitment without spending samples on whether to accept free gold.
fn strategic_learning_policy_candidates(
    boundary: &LearningStrategicBoundaryV1,
) -> Vec<&crate::ai::planner_core::LegalCandidate> {
    let candidates = &boundary.legal_candidates.candidates;
    let has_empty_potion_slot = boundary
        .observation
        .potions
        .iter()
        .any(|slot| slot.potion.is_none());
    let has_sozu = boundary
        .observation
        .relics
        .iter()
        .any(|relic| relic.relic == crate::content::relics::RelicId::Sozu);
    admitted_strategic_candidate_ordinals_v1(
        matches!(
            &boundary.observation.context,
            PlannerDecisionContext::Reward { .. }
        ),
        matches!(
            boundary.observation.decision_site,
            PlannerDecisionSite::Map | PlannerDecisionSite::CardReward
        ),
        has_empty_potion_slot,
        has_sozu,
        candidates.iter().enumerate().map(|(ordinal, candidate)| {
            let kind = match &candidate.action {
                PlannerAction::ClaimReward {
                    reward:
                        crate::ai::planner_core::PlannerRewardDescriptor::Gold { .. }
                        | crate::ai::planner_core::PlannerRewardDescriptor::StolenGold { .. },
                    ..
                } => PublicStrategicAdmissionKindV1::FreeGoldReward,
                PlannerAction::ClaimReward {
                    reward: crate::ai::planner_core::PlannerRewardDescriptor::Potion { .. },
                    ..
                } => PublicStrategicAdmissionKindV1::PotionReward,
                PlannerAction::OpenCardReward { .. } => {
                    PublicStrategicAdmissionKindV1::OpenCardReward
                }
                PlannerAction::DiscardRunPotion { .. } => {
                    PublicStrategicAdmissionKindV1::DiscardRunPotion
                }
                PlannerAction::Cancel { .. }
                    if boundary.observation.decision_site == PlannerDecisionSite::Map =>
                {
                    PublicStrategicAdmissionKindV1::MapCancel
                }
                _ => PublicStrategicAdmissionKindV1::Other,
            };
            (ordinal, kind)
        }),
    )
    .into_iter()
    .map(|ordinal| &candidates[ordinal])
    .collect()
}

/// A ragged batch of decisions.
///
/// `candidate_row_splits` is the zero-copy/default action-mask contract:
/// candidates in each half-open row range are legal and no padding exists.
/// Backends that require rectangular tensors may request [`dense_action_mask`].
#[derive(Clone, Debug)]
pub struct LearningModelBatchV1<'a> {
    pub decisions: Vec<LearningModelDecisionV1<'a>>,
    pub candidate_row_splits: Vec<usize>,
}

impl<'a> LearningModelBatchV1<'a> {
    pub fn from_boundaries(
        boundaries: &'a [LearningBoundaryV1],
    ) -> Result<Self, LearningModelInputError> {
        Self::from_boundary_refs(boundaries.iter())
    }

    pub fn from_boundary_refs(
        boundaries: impl IntoIterator<Item = &'a LearningBoundaryV1>,
    ) -> Result<Self, LearningModelInputError> {
        Self::from_decision_results(
            boundaries
                .into_iter()
                .map(LearningModelDecisionV1::from_boundary),
        )
    }

    pub fn from_combat_boundary_refs(
        boundaries: impl IntoIterator<Item = &'a LearningCombatBoundaryV1>,
    ) -> Result<Self, LearningModelInputError> {
        Self::from_combat_boundary_refs_with_potion_policy(
            boundaries,
            &CombatLearningPotionPolicyV1::All,
        )
    }

    pub fn from_combat_boundary_refs_with_potion_policy(
        boundaries: impl IntoIterator<Item = &'a LearningCombatBoundaryV1>,
        potion_policy: &CombatLearningPotionPolicyV1,
    ) -> Result<Self, LearningModelInputError> {
        Self::from_decision_results(boundaries.into_iter().map(|boundary| {
            LearningModelDecisionV1::from_combat_boundary_with_potion_policy(
                boundary,
                potion_policy,
            )
        }))
    }

    fn from_decision_results(
        decisions: impl IntoIterator<
            Item = Result<LearningModelDecisionV1<'a>, LearningModelInputError>,
        >,
    ) -> Result<Self, LearningModelInputError> {
        let decision_results = decisions.into_iter();
        let (lower_bound, _) = decision_results.size_hint();
        let mut decisions = Vec::with_capacity(lower_bound);
        let mut candidate_row_splits = Vec::with_capacity(lower_bound.saturating_add(1));
        candidate_row_splits.push(0);
        for decision in decision_results {
            let decision = decision?;
            let next = candidate_row_splits
                .last()
                .copied()
                .unwrap_or(0usize)
                .checked_add(decision.candidates.len())
                .ok_or(LearningModelInputError::CandidateCountOverflow)?;
            decisions.push(decision);
            candidate_row_splits.push(next);
        }
        Ok(Self {
            decisions,
            candidate_row_splits,
        })
    }

    pub(crate) fn from_decisions(
        decisions: Vec<LearningModelDecisionV1<'a>>,
    ) -> Result<Self, LearningModelInputError> {
        Self::from_decision_results(decisions.into_iter().map(Ok))
    }

    pub fn flattened_candidate_count(&self) -> usize {
        self.candidate_row_splits.last().copied().unwrap_or(0)
    }

    pub fn dense_action_mask(&self) -> LearningDenseActionMaskV1 {
        dense_action_mask(
            self.decisions
                .iter()
                .map(|decision| decision.candidates.len()),
        )
    }
}

/// Batched autoregressive selection decisions.
///
/// Each row carries the unchanged parent observation alongside the current
/// append-or-submit candidates, so a backend can batch symbolic decoding
/// across environment slots without losing state context.
#[derive(Clone, Debug)]
pub struct LearningSelectionModelBatchV1<'a> {
    pub rows: Vec<LearningSelectionModelRowV1<'a>>,
    pub candidate_row_splits: Vec<usize>,
}

impl<'a> LearningSelectionModelBatchV1<'a> {
    pub fn from_rows(
        rows: impl IntoIterator<Item = (LearningModelObservationV1<'a>, &'a LearningSelectionDraftV1)>,
    ) -> Result<Self, LearningModelInputError> {
        let rows = rows.into_iter();
        let (lower_bound, _) = rows.size_hint();
        let mut batch_rows = Vec::with_capacity(lower_bound);
        let mut candidate_row_splits = Vec::with_capacity(lower_bound.saturating_add(1));
        candidate_row_splits.push(0);
        for (observation, draft) in rows {
            let decision = draft.decision();
            ensure_nonempty(decision.candidates.len())?;
            let next = candidate_row_splits
                .last()
                .copied()
                .unwrap_or(0usize)
                .checked_add(decision.candidates.len())
                .ok_or(LearningModelInputError::CandidateCountOverflow)?;
            batch_rows.push(LearningSelectionModelRowV1 {
                observation,
                decision,
            });
            candidate_row_splits.push(next);
        }
        Ok(Self {
            rows: batch_rows,
            candidate_row_splits,
        })
    }

    pub fn flattened_candidate_count(&self) -> usize {
        self.candidate_row_splits.last().copied().unwrap_or(0)
    }

    pub fn dense_action_mask(&self) -> LearningDenseActionMaskV1 {
        dense_action_mask(self.rows.iter().map(|row| row.decision.candidates.len()))
    }
}

#[derive(Clone, Debug)]
pub struct LearningSelectionModelRowV1<'a> {
    pub observation: LearningModelObservationV1<'a>,
    pub decision: LearningSelectionDecisionV1,
}

fn dense_action_mask(
    candidate_counts: impl IntoIterator<Item = usize>,
) -> LearningDenseActionMaskV1 {
    let candidate_counts = candidate_counts.into_iter().collect::<Vec<_>>();
    let width = candidate_counts.iter().copied().max().unwrap_or(0);
    let mut values = vec![false; candidate_counts.len().saturating_mul(width)];
    for (row, candidate_count) in candidate_counts.iter().copied().enumerate() {
        let start = row * width;
        values[start..start + candidate_count].fill(true);
    }
    LearningDenseActionMaskV1 {
        rows: candidate_counts.len(),
        width,
        values,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningDenseActionMaskV1 {
    pub rows: usize,
    pub width: usize,
    /// Row-major mask. Real ragged candidates are true; padding is false.
    pub values: Vec<bool>,
}

#[derive(Clone, Debug)]
pub enum LearningModelChoiceV1 {
    Apply(LearningActionV1),
    DecodeSelection(LearningSelectionDraftV1),
}

/// An uncommitted symbolic selection.
///
/// Appending to this draft does not step the simulator. Only selecting the
/// explicit submit candidate produces a [`LearningActionV1`].
#[derive(Clone)]
enum LearningSelectionFamilyStateV1 {
    Combat {
        public: PublicCombatSelectionFamilyV1,
        private_resolution: CombatSelectionFamilyResolutionV1,
    },
    Run(LearningRunSelectionFamilyStateV1),
}

#[derive(Clone)]
struct LearningRunSelectionFamilyStateV1 {
    planner_candidate_id: String,
    scope: SelectionScope,
    reason: SelectionReason,
    min_choices: usize,
    max_choices: usize,
    selectable_card_lineages: Vec<CardLineageId>,
    selectable_card_ordinals: Vec<usize>,
}

#[derive(Clone, Copy)]
pub struct LearningRunSelectionFamilyV1<'a> {
    family: &'a LearningRunSelectionFamilyStateV1,
}

impl fmt::Debug for LearningRunSelectionFamilyV1<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LearningRunSelectionFamilyV1")
            .field("scope", &self.scope())
            .field("reason", &self.reason())
            .field("declared_min", &self.declared_min())
            .field("declared_max", &self.declared_max())
            .field("effective_max", &self.effective_max())
            .field("domain_count", &self.domain_count())
            .finish()
    }
}

impl LearningRunSelectionFamilyV1<'_> {
    pub fn scope(&self) -> SelectionScope {
        self.family.scope
    }

    pub fn reason(&self) -> SelectionReason {
        self.family.reason
    }

    pub fn declared_min(&self) -> usize {
        self.family.min_choices
    }

    pub fn declared_max(&self) -> usize {
        self.family.max_choices
    }

    pub fn effective_max(&self) -> usize {
        self.family
            .max_choices
            .min(self.family.selectable_card_lineages.len())
    }

    pub fn domain_count(&self) -> usize {
        self.family.selectable_card_lineages.len()
    }

    pub fn domain_card_ordinal(&self, index: usize) -> Option<usize> {
        self.family.selectable_card_ordinals.get(index).copied()
    }
}

#[derive(Clone)]
pub struct LearningSelectionDraftV1 {
    family: LearningSelectionFamilyStateV1,
    selected_domain_indices: Vec<usize>,
}

impl fmt::Debug for LearningSelectionDraftV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LearningSelectionDraftV1")
            .field("combat_family", &self.combat_family())
            .field("run_family", &self.run_family())
            .field("selected_domain_indices", &self.selected_domain_indices)
            .finish()
    }
}

impl LearningSelectionDraftV1 {
    fn from_combat(
        public: PublicCombatSelectionFamilyV1,
        private_resolution: CombatSelectionFamilyResolutionV1,
    ) -> Self {
        Self {
            family: LearningSelectionFamilyStateV1::Combat {
                public,
                private_resolution,
            },
            selected_domain_indices: Vec::new(),
        }
    }

    fn from_run(
        planner_candidate_id: String,
        scope: SelectionScope,
        reason: SelectionReason,
        min_choices: usize,
        max_choices: usize,
        selectable_card_lineages: Vec<CardLineageId>,
        selectable_card_ordinals: Vec<usize>,
    ) -> Self {
        Self {
            family: LearningSelectionFamilyStateV1::Run(LearningRunSelectionFamilyStateV1 {
                planner_candidate_id,
                scope,
                reason,
                min_choices,
                max_choices,
                selectable_card_lineages,
                selectable_card_ordinals,
            }),
            selected_domain_indices: Vec::new(),
        }
    }

    pub fn combat_family(&self) -> Option<LearningCombatSelectionFamilyV1<'_>> {
        match &self.family {
            LearningSelectionFamilyStateV1::Combat { public, .. } => {
                Some(LearningCombatSelectionFamilyV1 { family: public })
            }
            LearningSelectionFamilyStateV1::Run(_) => None,
        }
    }

    pub fn run_family(&self) -> Option<LearningRunSelectionFamilyV1<'_>> {
        match &self.family {
            LearningSelectionFamilyStateV1::Combat { .. } => None,
            LearningSelectionFamilyStateV1::Run(family) => {
                Some(LearningRunSelectionFamilyV1 { family })
            }
        }
    }

    pub fn selected_domain_indices(&self) -> &[usize] {
        &self.selected_domain_indices
    }

    pub fn decision(&self) -> LearningSelectionDecisionV1 {
        let mut candidates = Vec::new();
        if self.can_submit() {
            candidates.push(LearningSelectionCandidateV1 {
                semantics: LearningSelectionCandidateSemanticsV1::Submit,
                resolution: LearningSelectionCandidateResolutionV1::Submit,
            });
        }
        let domain_count = match &self.family {
            LearningSelectionFamilyStateV1::Combat { public, .. } => public.domain.len(),
            LearningSelectionFamilyStateV1::Run(family) => family.selectable_card_lineages.len(),
        };
        for domain_index in 0..domain_count {
            if self.can_append(domain_index) {
                candidates.push(LearningSelectionCandidateV1 {
                    semantics: LearningSelectionCandidateSemanticsV1::Append { domain_index },
                    resolution: LearningSelectionCandidateResolutionV1::Append { domain_index },
                });
            }
        }
        LearningSelectionDecisionV1 { candidates }
    }

    pub fn choose(
        &mut self,
        candidate_ordinal: usize,
    ) -> Result<LearningSelectionStepV1, LearningModelInputError> {
        let decision = self.decision();
        let candidate = decision.candidates.get(candidate_ordinal).ok_or(
            LearningModelInputError::SelectionCandidateOrdinalOutOfRange {
                candidate_ordinal,
                candidate_count: decision.candidates.len(),
            },
        )?;
        match candidate.resolution {
            LearningSelectionCandidateResolutionV1::Append { domain_index } => {
                self.selected_domain_indices.push(domain_index);
                Ok(LearningSelectionStepV1::Continue)
            }
            LearningSelectionCandidateResolutionV1::Submit => {
                Ok(LearningSelectionStepV1::Apply(self.to_learning_action()?))
            }
        }
    }

    fn can_submit(&self) -> bool {
        match &self.family {
            LearningSelectionFamilyStateV1::Combat { public, .. } => {
                self.selected_domain_indices.len() >= u64_to_usize(public.declared_min)
                    && self.selected_domain_indices.len() <= u64_to_usize(public.effective_max)
            }
            LearningSelectionFamilyStateV1::Run(family) => {
                self.selected_domain_indices.len() >= family.min_choices
                    && self.selected_domain_indices.len()
                        <= family
                            .max_choices
                            .min(family.selectable_card_lineages.len())
            }
        }
    }

    fn can_append(&self, domain_index: usize) -> bool {
        match &self.family {
            LearningSelectionFamilyStateV1::Combat {
                public,
                private_resolution,
            } => {
                if self.selected_domain_indices.len() >= u64_to_usize(public.effective_max) {
                    return false;
                }
                let Some(candidate) = public.domain.get(domain_index) else {
                    return false;
                };
                domain_candidate_is_eligible(candidate)
                    && !self
                        .selected_domain_indices
                        .iter()
                        .copied()
                        .any(|selected| {
                            same_selection_identity(
                                public,
                                private_resolution,
                                selected,
                                domain_index,
                            )
                        })
            }
            LearningSelectionFamilyStateV1::Run(family) => {
                if self.selected_domain_indices.len()
                    >= family
                        .max_choices
                        .min(family.selectable_card_lineages.len())
                {
                    return false;
                }
                let Some(candidate_lineage) = family.selectable_card_lineages.get(domain_index)
                else {
                    return false;
                };
                !self.selected_domain_indices.iter().any(|selected| {
                    family.selectable_card_lineages.get(*selected) == Some(candidate_lineage)
                })
            }
        }
    }

    fn to_learning_action(&self) -> Result<LearningActionV1, LearningModelInputError> {
        if !self.can_submit() {
            return Err(LearningModelInputError::SelectionCannotSubmit);
        }
        match &self.family {
            LearningSelectionFamilyStateV1::Combat {
                public,
                private_resolution,
            } => {
                let input = match public.input_encoding {
                    CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages => {
                        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                            SelectionScope::Hand,
                            self.selected_combat_card_lineages(private_resolution)?,
                        ))
                    }
                    CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages => {
                        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                            SelectionScope::Grid,
                            self.selected_combat_card_lineages(private_resolution)?,
                        ))
                    }
                    CombatSelectionInputEncodingV2::SubmitScryDiscardIndices => {
                        ClientInput::SubmitScryDiscard(
                            self.selected_scry_indices(private_resolution)?,
                        )
                    }
                };
                Ok(LearningActionV1::CombatInput { input })
            }
            LearningSelectionFamilyStateV1::Run(family) => {
                let selected = self
                    .selected_domain_indices
                    .iter()
                    .map(|index| {
                        family
                            .selectable_card_lineages
                            .get(*index)
                            .copied()
                            .ok_or(LearningModelInputError::SelectionDomainEncodingMismatch)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(LearningActionV1::RunSelection {
                    candidate_id: family.planner_candidate_id.clone(),
                    resolution: SelectionResolution::card_lineages(family.scope, selected),
                })
            }
        }
    }

    fn selected_combat_card_lineages(
        &self,
        family: &CombatSelectionFamilyResolutionV1,
    ) -> Result<Vec<CardLineageId>, LearningModelInputError> {
        self.selected_domain_indices
            .iter()
            .map(|index| match family.domain.get(*index) {
                Some(CombatSelectionDomainResolutionV1::Card { lineage }) => Ok(*lineage),
                _ => Err(LearningModelInputError::SelectionDomainEncodingMismatch),
            })
            .collect()
    }

    fn selected_scry_indices(
        &self,
        family: &CombatSelectionFamilyResolutionV1,
    ) -> Result<Vec<usize>, LearningModelInputError> {
        self.selected_domain_indices
            .iter()
            .map(|index| match family.domain.get(*index) {
                Some(CombatSelectionDomainResolutionV1::Scry { index, .. }) => {
                    usize::try_from(*index)
                        .map_err(|_| LearningModelInputError::SelectionIndexOverflow)
                }
                _ => Err(LearningModelInputError::SelectionDomainEncodingMismatch),
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LearningSelectionCandidateSemanticsV1 {
    Submit,
    Append { domain_index: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct LearningSelectionCandidateV1 {
    pub semantics: LearningSelectionCandidateSemanticsV1,
    resolution: LearningSelectionCandidateResolutionV1,
}

#[derive(Clone, Copy, Debug)]
enum LearningSelectionCandidateResolutionV1 {
    Submit,
    Append { domain_index: usize },
}

#[derive(Clone, Debug)]
pub struct LearningSelectionDecisionV1 {
    pub candidates: Vec<LearningSelectionCandidateV1>,
}

#[derive(Clone, Debug)]
pub enum LearningSelectionStepV1 {
    Continue,
    Apply(LearningActionV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LearningModelInputError {
    TerminalBoundary,
    UnsupportedBoundary,
    IncompleteStrategicCandidateSet {
        site: PlannerDecisionSite,
        gaps: Vec<CandidateRepresentationGap>,
    },
    StrategicObservationIdentityMismatch,
    StrategicDecisionSiteMismatch,
    StrategicMechanicsMismatch,
    StrategicCardTargetMissing,
    StrategicCardTargetAmbiguous,
    StrategicCardTargetMismatch,
    IndexedChoiceMetadataMissing,
    AtomicActionResolutionCountMismatch,
    SelectionFamilyResolutionCountMismatch,
    SelectionDomainResolutionCountMismatch,
    NoLegalCandidates,
    CandidateCountOverflow,
    CandidateOrdinalOutOfRange {
        candidate_ordinal: usize,
        candidate_count: usize,
    },
    SelectionCandidateOrdinalOutOfRange {
        candidate_ordinal: usize,
        candidate_count: usize,
    },
    SelectionCannotSubmit,
    SelectionDomainEncodingMismatch,
    SelectionIndexOverflow,
    EmptyDecisionPath,
    DecisionPathEndsBeforeApply,
    DecisionPathContinuesAfterApply,
}

impl fmt::Display for LearningModelInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for LearningModelInputError {}

fn ensure_nonempty(candidate_count: usize) -> Result<(), LearningModelInputError> {
    if candidate_count == 0 {
        Err(LearningModelInputError::NoLegalCandidates)
    } else {
        Ok(())
    }
}

fn validate_public_private_action_alignment(
    boundary: &LearningCombatBoundaryV1,
) -> Result<(), LearningModelInputError> {
    if boundary.public_actions.atomic_actions.len()
        != boundary.private_resolution.atomic_inputs.len()
    {
        return Err(LearningModelInputError::AtomicActionResolutionCountMismatch);
    }
    if boundary.public_actions.selection_families.len()
        != boundary.private_resolution.selection_families.len()
    {
        return Err(LearningModelInputError::SelectionFamilyResolutionCountMismatch);
    }
    for (public, private_resolution) in boundary
        .public_actions
        .selection_families
        .iter()
        .zip(&boundary.private_resolution.selection_families)
    {
        if public.domain.len() != private_resolution.domain.len() {
            return Err(LearningModelInputError::SelectionDomainResolutionCountMismatch);
        }
    }
    Ok(())
}

fn indexed_choice_semantics<'a>(
    boundary: &'a LearningCombatBoundaryV1,
    action: &'a PublicCombatAtomicActionV1,
) -> Option<LearningCombatIndexedChoiceV1<'a>> {
    let PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index } = action else {
        return None;
    };
    let indexed = boundary.public_actions.indexed_choice.as_ref()?;
    Some(LearningCombatIndexedChoiceV1 {
        input_encoding: indexed.input_encoding,
        reason: &indexed.reason,
        candidate: indexed.candidates.get(*choice_index)?,
    })
}

fn learning_atomic_semantics<'a>(
    boundary: &'a LearningCombatBoundaryV1,
    action: &'a PublicCombatAtomicActionV1,
) -> Result<LearningCombatAtomicActionV1<'a>, LearningModelInputError> {
    match action {
        PublicCombatAtomicActionV1::PlayCard {
            hand_index,
            target_monster_index,
        } => Ok(LearningCombatAtomicActionV1::PlayCard {
            hand_index: *hand_index,
            target_monster_index: *target_monster_index,
        }),
        PublicCombatAtomicActionV1::UsePotion {
            potion_index,
            target_monster_index,
        } => Ok(LearningCombatAtomicActionV1::UsePotion {
            potion_index: *potion_index,
            target_monster_index: *target_monster_index,
        }),
        PublicCombatAtomicActionV1::DiscardPotion { potion_index } => {
            Ok(LearningCombatAtomicActionV1::DiscardPotion {
                potion_index: *potion_index,
            })
        }
        PublicCombatAtomicActionV1::EndTurn => Ok(LearningCombatAtomicActionV1::EndTurn),
        PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index } => {
            Ok(LearningCombatAtomicActionV1::SubmitIndexedChoice {
                choice_index: *choice_index,
                indexed: indexed_choice_semantics(boundary, action)
                    .ok_or(LearningModelInputError::IndexedChoiceMetadataMissing)?,
            })
        }
        PublicCombatAtomicActionV1::Proceed => Ok(LearningCombatAtomicActionV1::Proceed),
        PublicCombatAtomicActionV1::Cancel => Ok(LearningCombatAtomicActionV1::Cancel),
    }
}

fn domain_candidate_is_eligible(candidate: &PublicCombatSelectionDomainCandidateV1) -> bool {
    match candidate {
        PublicCombatSelectionDomainCandidateV1::Card { eligible, .. } => *eligible,
        PublicCombatSelectionDomainCandidateV1::Scry {
            currently_present, ..
        } => *currently_present,
    }
}

fn same_selection_identity(
    family: &PublicCombatSelectionFamilyV1,
    private_resolution: &CombatSelectionFamilyResolutionV1,
    left_index: usize,
    right_index: usize,
) -> bool {
    let (Some(left), Some(right)) = (
        family.domain.get(left_index),
        family.domain.get(right_index),
    ) else {
        return false;
    };
    match family.distinct_by {
        PublicCombatSelectionDistinctByV1::CardOccurrence => match (left, right) {
            (
                PublicCombatSelectionDomainCandidateV1::Card { ordinal: left, .. },
                PublicCombatSelectionDomainCandidateV1::Card { ordinal: right, .. },
            ) => left == right,
            _ => false,
        },
        PublicCombatSelectionDistinctByV1::ScryOccurrence => {
            let same_public_index = match (left, right) {
                (
                    PublicCombatSelectionDomainCandidateV1::Scry {
                        index: left_index, ..
                    },
                    PublicCombatSelectionDomainCandidateV1::Scry {
                        index: right_index, ..
                    },
                ) => left_index == right_index,
                _ => false,
            };
            let same_private_card = match (
                private_resolution.domain.get(left_index),
                private_resolution.domain.get(right_index),
            ) {
                (
                    Some(CombatSelectionDomainResolutionV1::Scry {
                        card_lineage: Some(left),
                        ..
                    }),
                    Some(CombatSelectionDomainResolutionV1::Scry {
                        card_lineage: Some(right),
                        ..
                    }),
                ) => left == right,
                _ => false,
            };
            same_public_index || same_private_card
        }
    }
}

fn u64_to_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::planner_core::PublicDecisionDomainV1;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::{CombatCard, CombatState};
    use crate::sim::combat_action_surface::combat_legal_action_surface_v2;
    use crate::state::core::{
        ActiveCombat, CombatContext, EngineState, HandSelectReason, RoomCombatContext,
        RunPendingChoiceReason, RunPendingChoiceState,
    };
    use crate::state::map::node::RoomType;
    use crate::state::rewards::{RewardCard, RewardItem, RewardState};
    use crate::state::selection::DomainEventSource;
    use crate::state::PendingChoice;

    use super::super::{
        CombatLearningRootV1, LearningEnvV3, RunControlConfig, RunControlSession,
        RunControlSessionCheckpointV1,
    };

    fn legal_and_policy_atomic_inputs(
        mut combat: CombatState,
        potion_slots: Option<Vec<usize>>,
    ) -> (Vec<ClientInput>, Vec<ClientInput>) {
        combat
            .entities
            .monsters
            .push(crate::test_support::test_monster(EnemyId::JawWorm));
        let legal =
            combat_legal_action_surface_v2(&EngineState::CombatPlayerTurn, &combat).atomic_actions;
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let potion_policy = match potion_slots {
            None => CombatLearningPotionPolicyV1::All,
            Some(slots) => CombatLearningPotionPolicyV1::from_root_slots(
                &CombatLearningRootV1::from_checkpoint(
                    RunControlSessionCheckpointV1::from_session(&session),
                )
                .expect("combat root"),
                slots,
            )
            .expect("potion slots"),
        };
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let LearningBoundaryV1::Combat { boundary } = &boundary else {
            panic!("expected combat boundary");
        };
        let decision = LearningModelDecisionV1::from_combat_boundary_with_potion_policy(
            boundary,
            &potion_policy,
        )
        .expect("combat model decision");
        let policy = decision
            .candidates
            .iter()
            .filter_map(|candidate| match &candidate.resolution {
                LearningCandidateResolutionV1::CombatAtomic { input } => Some((*input).clone()),
                _ => None,
            })
            .collect();
        (legal, policy)
    }

    fn combat_env_for_model_input(combat: CombatState) -> LearningEnvV3 {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        LearningEnvV3::from_session(session)
    }

    #[test]
    fn combat_information_snapshot_ignores_hidden_draw_order_but_respects_frozen_eye() {
        fn combat_with_draw(draw: Vec<CombatCard>, frozen_eye: bool) -> CombatState {
            let mut combat = crate::test_support::blank_test_combat();
            combat
                .entities
                .monsters
                .push(crate::test_support::test_monster(EnemyId::JawWorm));
            combat.zones.hand = vec![
                CombatCard::for_scenario(CardId::Defend, 10),
                CombatCard::for_scenario(CardId::Strike, 11),
            ];
            combat.zones.draw_pile = draw.into();
            if frozen_eye {
                combat
                    .entities
                    .player
                    .add_relic(RelicState::new(RelicId::FrozenEye));
            }
            combat
        }

        let forward = vec![
            CombatCard::for_scenario(CardId::Bash, 20),
            CombatCard::for_scenario(CardId::Defend, 21),
        ];
        let reverse = vec![
            CombatCard::for_scenario(CardId::Defend, 21),
            CombatCard::for_scenario(CardId::Bash, 20),
        ];
        let hidden_forward = combat_env_for_model_input(combat_with_draw(forward.clone(), false))
            .observe()
            .expect("hidden-order forward boundary");
        let hidden_reverse = combat_env_for_model_input(combat_with_draw(reverse.clone(), false))
            .observe()
            .expect("hidden-order reverse boundary");
        let visible_forward = combat_env_for_model_input(combat_with_draw(forward, true))
            .observe()
            .expect("visible-order forward boundary");
        let visible_reverse = combat_env_for_model_input(combat_with_draw(reverse, true))
            .observe()
            .expect("visible-order reverse boundary");

        let hidden_forward = super::super::learning_public_information_snapshot_v1(&hidden_forward)
            .expect("hidden-order forward public snapshot");
        let hidden_reverse = super::super::learning_public_information_snapshot_v1(&hidden_reverse)
            .expect("hidden-order reverse public snapshot");
        let visible_forward =
            super::super::learning_public_information_snapshot_v1(&visible_forward)
                .expect("visible-order forward public snapshot");
        let visible_reverse =
            super::super::learning_public_information_snapshot_v1(&visible_reverse)
                .expect("visible-order reverse public snapshot");

        assert_eq!(hidden_forward.snapshot_id, hidden_reverse.snapshot_id);
        assert_ne!(visible_forward.snapshot_id, visible_reverse.snapshot_id);
        assert_eq!(hidden_forward.domain, PublicDecisionDomainV1::Combat);
        assert!(!hidden_forward
            .history_snapshot
            .history_snapshot_id
            .is_empty());
    }

    #[test]
    fn combat_information_snapshot_ignores_private_runtime_identities() {
        fn snapshot(
            monster_entity_id: usize,
            potion_uuid: u32,
            card_lineage: CardLineageId,
        ) -> crate::ai::planner_core::PublicInformationSnapshotV1 {
            let mut combat = crate::test_support::blank_test_combat();
            let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
            monster.id = monster_entity_id;
            combat.entities.monsters.push(monster);
            combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, potion_uuid))];
            combat.zones.hand = vec![CombatCard::with_lineage(CardId::Bash, card_lineage)];
            let boundary = combat_env_for_model_input(combat)
                .observe()
                .expect("combat boundary");
            super::super::learning_public_information_snapshot_v1(&boundary)
                .expect("public snapshot")
        }

        let first = snapshot(
            3_000_000_001,
            101,
            crate::test_support::scenario_lineage(77),
        );
        let same_public_state = snapshot(
            3_000_000_099,
            909,
            crate::test_support::scenario_lineage(707),
        );

        assert_eq!(first.observation, same_public_state.observation);
        assert_eq!(first.candidate_surface, same_public_state.candidate_surface);
        assert_eq!(first.snapshot_id, same_public_state.snapshot_id);
    }

    #[test]
    fn strategic_information_snapshot_ignores_private_card_and_potion_uuids() {
        fn snapshot(
            card_lineage: CardLineageId,
            potion_uuid: u32,
        ) -> crate::ai::planner_core::PublicInformationSnapshotV1 {
            use crate::ai::planner_core::{
                CandidateCompletenessBasis, LegalCandidate, LegalCandidateSet,
                PlannerCardObservation, PlannerDecisionContext, PlannerDecisionSite,
                PlannerMechanicsManifest, PlannerObservation, PlannerPlayerClass,
                PlannerPotionObservation, PlannerPotionSlotObservation, PlannerPublicHistory,
                PlannerPublicMap, PlannerRunGoal, PlannerRunScalars,
            };

            let mechanics = PlannerMechanicsManifest {
                mechanics_id: "test-mechanics".into(),
                mechanics_version: 1,
            };
            let observation_id = format!("private-observation-{card_lineage}-{potion_uuid}");
            let action = PlannerAction::Smith {
                card_lineage,
                card: CardId::Strike,
                upgrades: 0,
            };
            let boundary = LearningBoundaryV1::Strategic {
                boundary: LearningStrategicBoundaryV1 {
                    observation: PlannerObservation {
                        schema_name: "PlannerObservation".into(),
                        schema_version: 1,
                        observation_id: observation_id.clone(),
                        mechanics: mechanics.clone(),
                        run_goal: PlannerRunGoal::HeartVictory,
                        decision_site: PlannerDecisionSite::Campfire,
                        run: PlannerRunScalars {
                            player_class: PlannerPlayerClass::Ironclad,
                            ascension_level: 20,
                            act: 1,
                            floor: 8,
                            current_hp: 50,
                            max_hp: 80,
                            gold: 99,
                            keys: [false; 3],
                            potion_capacity: 1,
                            pending_act_entry: None,
                        },
                        cards: vec![PlannerCardObservation {
                            card_lineage,
                            card: CardId::Strike,
                            upgrades: 0,
                            misc_value: 0,
                            base_damage_override: None,
                            base_block_override: None,
                            cost_modifier: 0,
                        }],
                        relics: Vec::new(),
                        potions: vec![PlannerPotionSlotObservation {
                            slot: 0,
                            potion: Some(PlannerPotionObservation {
                                potion: PotionId::BlockPotion,
                                potion_uuid,
                                can_use: true,
                                can_discard: true,
                                requires_target: false,
                            }),
                        }],
                        public_map: PlannerPublicMap {
                            current_x: 0,
                            current_y: 7,
                            boss: None,
                            nodes: Vec::new(),
                        },
                        context: PlannerDecisionContext::Campfire,
                        public_history: PlannerPublicHistory {
                            shop_purge_count: 0,
                            recent_combat_hp_loss: None,
                            recent_combat_turns: None,
                            recent_combat_potions_used: None,
                        },
                    },
                    legal_candidates: LegalCandidateSet {
                        schema_name: "LegalCandidateSet".into(),
                        schema_version: 1,
                        candidate_set_id: format!("private-candidate-set-{card_lineage}"),
                        decision_id: format!("private-decision-{card_lineage}"),
                        observation_id,
                        site: PlannerDecisionSite::Campfire,
                        candidates: vec![LegalCandidate {
                            candidate_id: format!("private-candidate-{card_lineage}"),
                            action,
                            mechanics,
                        }],
                        completeness: CandidateSetCompleteness::Complete {
                            basis: CandidateCompletenessBasis::RunControlBoundaryEnumerator,
                        },
                    },
                },
            };
            super::super::learning_public_information_snapshot_v1(&boundary)
                .expect("strategic public snapshot")
        }

        let first = snapshot(crate::test_support::scenario_lineage(11), 21);
        let same_public_state = snapshot(crate::test_support::scenario_lineage(101), 202);

        assert_eq!(first.observation, same_public_state.observation);
        assert_eq!(first.candidate_surface, same_public_state.candidate_surface);
        assert_eq!(first.snapshot_id, same_public_state.snapshot_id);
    }

    #[test]
    fn duplicate_defends_preserve_each_visible_hand_occurrence() {
        let mut combat = crate::test_support::blank_test_combat();
        combat
            .entities
            .monsters
            .push(crate::test_support::test_monster(EnemyId::JawWorm));
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let env = combat_env_for_model_input(combat);
        let boundary = env.observe().expect("combat boundary");
        let decision = LearningModelDecisionV1::from_boundary(&boundary).expect("model decision");
        let LearningModelObservationV1::Combat(observation) = decision.observation else {
            panic!("expected combat observation");
        };
        let defend_candidates = decision
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(ordinal, candidate)| {
                let LearningModelCandidateSemanticsV1::CombatAtomic {
                    action:
                        LearningCombatAtomicActionV1::PlayCard {
                            hand_index,
                            target_monster_index: None,
                        },
                } = candidate.semantics
                else {
                    return None;
                };
                (observation.cards.hand.cards[hand_index].card_id == CardId::Defend)
                    .then_some((ordinal, hand_index))
            })
            .collect::<Vec<_>>();

        assert_eq!(defend_candidates.len(), 2);
        assert_eq!(
            defend_candidates
                .iter()
                .map(|(_, hand_index)| *hand_index)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        for (ordinal, _) in defend_candidates {
            let LearningModelChoiceV1::Apply(action) =
                decision.choose(ordinal).expect("choose visible occurrence")
            else {
                panic!("defend must resolve directly");
            };
            env.prepare_action(action)
                .expect("visible occurrence must remain a legal engine input");
        }
    }

    #[test]
    fn starter_basic_equivalence_preserves_targets_and_runtime_state() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut first = crate::test_support::test_monster(EnemyId::LouseNormal);
        first.id = 1;
        let mut second = crate::test_support::test_monster(EnemyId::LouseNormal);
        second.id = 2;
        combat.entities.monsters = vec![first, second];
        let mut free = CombatCard::for_scenario(CardId::Strike, 11);
        free.free_to_play_once = true;
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10), free];
        let env = combat_env_for_model_input(combat);
        let boundary = env.observe().expect("combat boundary");
        let decision = LearningModelDecisionV1::from_boundary(&boundary).expect("model decision");
        let strike_candidates = decision
            .candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.semantics,
                    LearningModelCandidateSemanticsV1::CombatAtomic {
                        action: LearningCombatAtomicActionV1::PlayCard { .. },
                    }
                )
            })
            .count();

        assert_eq!(strike_candidates, 4);
    }

    #[test]
    fn combat_policy_withholds_potion_discard_without_an_immediate_refill() {
        let mut ordinary = crate::test_support::blank_test_combat();
        ordinary.entities.potions = vec![
            Some(Potion::new(PotionId::BlockPotion, 1)),
            Some(Potion::new(PotionId::SkillPotion, 2)),
        ];
        let (legal, policy) = legal_and_policy_atomic_inputs(ordinary, None);
        assert!(legal.contains(&ClientInput::DiscardPotion(0)));
        assert!(legal.contains(&ClientInput::DiscardPotion(1)));
        assert!(!policy
            .iter()
            .any(|input| matches!(input, ClientInput::DiscardPotion(_))));

        let mut brew = crate::test_support::blank_test_combat();
        brew.entities.potions = vec![
            Some(Potion::new(PotionId::EntropicBrew, 3)),
            Some(Potion::new(PotionId::BlockPotion, 4)),
        ];
        let (_, policy) = legal_and_policy_atomic_inputs(brew, None);
        assert!(!policy.contains(&ClientInput::DiscardPotion(0)));
        assert!(policy.contains(&ClientInput::DiscardPotion(1)));

        let mut alchemize = crate::test_support::blank_test_combat();
        alchemize.entities.potions = vec![
            Some(Potion::new(PotionId::BlockPotion, 5)),
            Some(Potion::new(PotionId::SkillPotion, 6)),
        ];
        alchemize.zones.hand = vec![CombatCard::for_scenario(CardId::Alchemize, 7)];
        let (_, policy) = legal_and_policy_atomic_inputs(alchemize, None);
        assert!(policy.contains(&ClientInput::DiscardPotion(0)));
        assert!(policy.contains(&ClientInput::DiscardPotion(1)));

        let mut sozu = crate::test_support::blank_test_combat();
        sozu.entities.potions = vec![
            Some(Potion::new(PotionId::EntropicBrew, 8)),
            Some(Potion::new(PotionId::BlockPotion, 9)),
        ];
        sozu.entities
            .player
            .add_relic(RelicState::new(RelicId::Sozu));
        let (_, policy) = legal_and_policy_atomic_inputs(sozu, None);
        assert!(!policy
            .iter()
            .any(|input| matches!(input, ClientInput::DiscardPotion(_))));

        let mut no_potions = crate::test_support::blank_test_combat();
        no_potions.entities.potions = vec![
            Some(Potion::new(PotionId::EntropicBrew, 10)),
            Some(Potion::new(PotionId::BlockPotion, 11)),
        ];
        no_potions.zones.hand = vec![CombatCard::for_scenario(CardId::Alchemize, 12)];
        let (legal, policy) = legal_and_policy_atomic_inputs(no_potions, Some(Vec::new()));
        assert!(legal.iter().any(|input| matches!(
            input,
            ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
        )));
        assert!(!policy.iter().any(|input| matches!(
            input,
            ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
        )));

        let mut one_root_potion = crate::test_support::blank_test_combat();
        one_root_potion.entities.potions = vec![
            Some(Potion::new(PotionId::EntropicBrew, 13)),
            Some(Potion::new(PotionId::BlockPotion, 14)),
        ];
        let (_, policy) = legal_and_policy_atomic_inputs(one_root_potion, Some(vec![1]));
        assert!(policy.iter().any(|input| matches!(
            input,
            ClientInput::UsePotion {
                potion_index: 1,
                ..
            } | ClientInput::DiscardPotion(1)
        )));
        assert!(!policy.iter().any(|input| matches!(
            input,
            ClientInput::UsePotion {
                potion_index: 0,
                ..
            } | ClientInput::DiscardPotion(0)
        )));
        assert!(!policy.contains(&ClientInput::DiscardPotion(1)));
    }

    #[test]
    fn root_potion_policy_does_not_authorize_a_replacement_in_the_same_slot() {
        let mut combat = crate::test_support::blank_test_combat();
        combat
            .entities
            .monsters
            .push(crate::test_support::test_monster(EnemyId::JawWorm));
        combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 100))];
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let root = CombatLearningRootV1::from_checkpoint(
            RunControlSessionCheckpointV1::from_session(&session),
        )
        .expect("combat root");
        let policy =
            CombatLearningPotionPolicyV1::from_root_slots(&root, [0]).expect("root potion policy");

        session
            .active_combat
            .as_mut()
            .expect("active combat")
            .combat_state
            .entities
            .potions[0]
            .as_mut()
            .expect("replacement potion")
            .uuid = 101;
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("replacement boundary");
        let LearningBoundaryV1::Combat { boundary } = &boundary else {
            panic!("expected combat boundary");
        };
        assert!(boundary
            .private_resolution
            .atomic_inputs
            .iter()
            .any(|input| {
                matches!(
                    input,
                    ClientInput::UsePotion {
                        potion_index: 0,
                        ..
                    }
                )
            }));
        let decision =
            LearningModelDecisionV1::from_combat_boundary_with_potion_policy(boundary, &policy)
                .expect("model decision");
        assert!(!decision.candidates.iter().any(|candidate| matches!(
            candidate.resolution,
            LearningCandidateResolutionV1::CombatAtomic {
                input: ClientInput::UsePotion {
                    potion_index: 0,
                    ..
                }
            }
        )));
    }

    #[test]
    fn strategic_model_view_resolves_an_ordinal_without_exposing_candidate_ids() {
        let env = LearningEnvV3::new(RunControlConfig::default());
        let boundary = env.observe().expect("observe initial boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("build model decision");

        assert!(matches!(
            decision.observation,
            LearningModelObservationV1::Strategic(_)
        ));
        assert!(decision.candidates.iter().all(|candidate| matches!(
            candidate.semantics,
            LearningModelCandidateSemanticsV1::Strategic { .. }
        )));
        assert!(matches!(
            decision.choose(0).expect("resolve first ordinal"),
            LearningModelChoiceV1::Apply(LearningActionV1::StrategicCandidate { .. })
        ));
    }

    #[test]
    fn strategic_event_card_target_resolves_exact_occurrence_in_current_order() {
        let env = LearningEnvV3::new(RunControlConfig::default());
        let boundary = env.observe().unwrap();
        let decision = LearningModelDecisionV1::from_boundary(&boundary).unwrap();
        let LearningModelObservationV1::Strategic(observation) = decision.observation else {
            panic!("expected strategic observation");
        };
        let card = PlannerCardObservation {
            card_lineage: crate::test_support::scenario_lineage(11),
            card: CardId::Armaments,
            upgrades: 1,
            misc_value: 0,
            base_damage_override: None,
            base_block_override: None,
            cost_modifier: 0,
        };
        let duplicate_name = PlannerCardObservation {
            card_lineage: crate::test_support::scenario_lineage(12),
            ..card.clone()
        };
        let cards = [duplicate_name.clone(), card.clone()];
        let view = LearningStrategicModelObservationV1 {
            cards: &cards,
            ..observation
        };
        assert_eq!(view.card_effect_target_ordinal(&card), Ok(1));
        assert_eq!(view.card_effect_target_ordinal(&duplicate_name), Ok(0));

        let reordered = [card.clone(), duplicate_name];
        let view = LearningStrategicModelObservationV1 {
            cards: &reordered,
            ..observation
        };
        assert_eq!(view.card_effect_target_ordinal(&card), Ok(0));

        let missing = PlannerCardObservation {
            card_lineage: crate::test_support::scenario_lineage(13),
            ..card
        };
        assert_eq!(
            view.card_effect_target_ordinal(&missing),
            Err(LearningModelInputError::StrategicCardTargetMissing)
        );
    }

    #[test]
    fn strategic_event_card_target_rejects_stale_public_attributes_and_ambiguous_lineage() {
        let env = LearningEnvV3::new(RunControlConfig::default());
        let boundary = env.observe().unwrap();
        let decision = LearningModelDecisionV1::from_boundary(&boundary).unwrap();
        let LearningModelObservationV1::Strategic(observation) = decision.observation else {
            panic!("expected strategic observation");
        };
        let target = observation.cards[0].clone();
        let mut changed = vec![target.clone(); 6];
        changed[0].card = CardId::Armaments;
        changed[1].upgrades += 1;
        changed[2].misc_value += 1;
        changed[3].base_damage_override = Some(99);
        changed[4].base_block_override = Some(99);
        changed[5].cost_modifier += 1;
        for card in changed {
            let cards = [card.clone()];
            let view = LearningStrategicModelObservationV1 {
                cards: &cards,
                ..observation
            };
            assert_eq!(
                view.card_effect_target_ordinal(&target),
                Err(LearningModelInputError::StrategicCardTargetMismatch)
            );
            let ambiguous = [target.clone(), card];
            let view = LearningStrategicModelObservationV1 {
                cards: &ambiguous,
                ..observation
            };
            assert_eq!(
                view.card_effect_target_ordinal(&target),
                Err(LearningModelInputError::StrategicCardTargetAmbiguous)
            );
        }
    }

    #[test]
    fn combat_model_view_resolves_an_exact_atomic_input_ordinal() {
        let mut combat = crate::test_support::blank_test_combat();
        combat
            .entities
            .monsters
            .push(crate::test_support::test_monster(EnemyId::JawWorm));
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("combat model decision");

        let ordinal = decision
            .combat_atomic_ordinal_for_input(&ClientInput::EndTurn)
            .expect("end turn ordinal");
        assert!(matches!(
            decision.choose(ordinal).expect("resolve end turn"),
            LearningModelChoiceV1::Apply(LearningActionV1::CombatInput {
                input: ClientInput::EndTurn,
            })
        ));
        assert_eq!(
            decision.combat_atomic_ordinal_for_input(&ClientInput::PlayCard {
                card_index: usize::MAX,
                target: None,
            }),
            None
        );
    }

    #[test]
    fn strategic_reward_keeps_legal_discard_out_of_model_surface_with_empty_slot() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.potions = vec![Some(Potion::new(PotionId::Elixir, 1)), None];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Card {
            cards: vec![RewardCard::new(CardId::PommelStrike, 0)],
        }];
        session.engine_state = EngineState::RewardScreen(reward);
        let boundary = LearningEnvV3::from_session(session).observe().unwrap();
        let LearningBoundaryV1::Strategic {
            boundary: strategic_boundary,
        } = &boundary
        else {
            panic!("reward should expose a strategic decision");
        };
        assert!(strategic_boundary
            .legal_candidates
            .candidates
            .iter()
            .any(|candidate| {
                matches!(candidate.action, PlannerAction::DiscardRunPotion { .. })
            }));
        let decision = LearningModelDecisionV1::from_boundary(&boundary).unwrap();
        assert!(!decision.candidates.iter().any(|candidate| {
            matches!(candidate.semantics,
                LearningModelCandidateSemanticsV1::Strategic { action }
                    if matches!(action.semantics(),
                        Ok(LearningStrategicActionSemanticsV2::DiscardRunPotion { .. })))
        }));
    }

    #[test]
    fn legal_surface_preserves_reward_order_and_potion_discard_as_actual_choices() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.potions = vec![Some(Potion::new(PotionId::Elixir, 1)), None];
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Gold { amount: 12 },
            RewardItem::Potion {
                potion_id: PotionId::LiquidMemories,
            },
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::PommelStrike, 0)],
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let env = LearningEnvV3::from_session(session);
        let boundary = env.observe().unwrap();
        let legal = LearningModelDecisionV1::from_boundary_with_surface(
            &boundary,
            &CombatLearningPotionPolicyV1::All,
            LearningCandidateSurfaceV1::Legal,
        )
        .unwrap();
        let LearningBoundaryV1::Strategic { boundary: root } = &boundary else {
            panic!("strategic reward");
        };
        assert_eq!(
            legal.candidates.len(),
            root.legal_candidates.candidates.len()
        );
        let potion = legal.candidates.iter().position(|candidate| matches!(candidate.semantics,
            LearningModelCandidateSemanticsV1::Strategic { action }
            if matches!(action.semantics(), Ok(LearningStrategicActionSemanticsV2::ClaimReward { reward: crate::ai::planner_core::PlannerRewardDescriptor::Potion { .. }, .. }))));
        let Some(potion) = potion else {
            panic!("potion must remain available before gold");
        };
        let LearningModelChoiceV1::Apply(action) = legal.choose(potion).unwrap() else {
            panic!("potion is atomic");
        };
        let mut actual = env.clone();
        let prepared = actual.prepare_action(action).unwrap();
        actual.step_prepared(prepared).unwrap();
        let state = actual.into_session();
        assert_eq!(
            state.run_state.gold,
            env.clone().into_session().run_state.gold
        );
        assert_eq!(
            state.run_state.potions[1].as_ref().unwrap().id,
            PotionId::LiquidMemories
        );
        assert_eq!(
            LearningModelDecisionV1::from_boundary(&boundary)
                .unwrap()
                .candidates
                .len(),
            1
        );
    }

    #[test]
    fn strategic_card_reward_defers_discard_until_potion_acquisition_returns() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::SmokeBomb, 1)),
            Some(Potion::new(PotionId::BlockPotion, 2)),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::PommelStrike, 0)],
            },
            RewardItem::Potion {
                potion_id: PotionId::StrengthPotion,
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let mut env = LearningEnvV3::from_session(session);
        let apply = |env: &mut LearningEnvV3, action| {
            let prepared = env.prepare_action(action).unwrap();
            env.step_prepared(prepared).unwrap();
        };
        let semantics = |boundary: &LearningBoundaryV1| {
            LearningModelDecisionV1::from_boundary(boundary)
                .unwrap()
                .candidates
                .iter()
                .map(|candidate| match candidate.semantics {
                    LearningModelCandidateSemanticsV1::Strategic { action } => {
                        action.semantics().unwrap()
                    }
                    _ => panic!("expected strategic candidate"),
                })
                .collect::<Vec<_>>()
        };
        let boundary = env.observe().unwrap();
        let actions = semantics(&boundary);
        assert!(actions.iter().any(|action| matches!(
            action,
            LearningStrategicActionSemanticsV2::DiscardRunPotion { .. }
        )));
        let open = actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    LearningStrategicActionSemanticsV2::OpenCardReward { .. }
                )
            })
            .unwrap();
        let decision = LearningModelDecisionV1::from_boundary(&boundary).unwrap();
        let LearningModelChoiceV1::Apply(action) = decision.choose(open).unwrap() else {
            panic!("card reveal is atomic");
        };
        apply(&mut env, action);
        let boundary = env.observe().unwrap();
        let LearningBoundaryV1::Strategic { boundary: root } = &boundary else {
            panic!("card choice is strategic");
        };
        assert!(
            root.legal_candidates
                .candidates
                .iter()
                .any(|candidate| matches!(
                    candidate.action,
                    PlannerAction::DiscardRunPotion { .. }
                )),
            "manual runtime legality stays intact"
        );
        let actions = semantics(&boundary);
        assert_eq!(actions.len(), 2);
        assert!(!actions.iter().any(|action| matches!(
            action,
            LearningStrategicActionSemanticsV2::DiscardRunPotion { .. }
        )));
        let skip = actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    LearningStrategicActionSemanticsV2::SkipCardReward { .. }
                )
            })
            .unwrap();
        let decision = LearningModelDecisionV1::from_boundary(&boundary).unwrap();
        let LearningModelChoiceV1::Apply(action) = decision.choose(skip).unwrap() else {
            panic!("card skip is atomic");
        };
        apply(&mut env, action);
        let boundary = env.observe().unwrap();
        let actions = semantics(&boundary);
        let discard = actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    LearningStrategicActionSemanticsV2::DiscardRunPotion { slot: 1, .. }
                )
            })
            .unwrap();
        let decision = LearningModelDecisionV1::from_boundary(&boundary).unwrap();
        let LearningModelChoiceV1::Apply(action) = decision.choose(discard).unwrap() else {
            panic!("potion discard is atomic");
        };
        apply(&mut env, action);
        let boundary = env.observe().unwrap();
        let actions = semantics(&boundary);
        assert!(matches!(
            actions.as_slice(),
            [LearningStrategicActionSemanticsV2::ClaimReward {
                reward: crate::ai::planner_core::PlannerRewardDescriptor::Potion {
                    potion: PotionId::StrengthPotion
                },
                ..
            }]
        ));
    }

    #[test]
    fn strategic_reward_policy_forces_free_resources_but_keeps_reveal_and_exit() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Gold { amount: 12 },
            RewardItem::Potion {
                potion_id: PotionId::SmokeBomb,
            },
            RewardItem::Card {
                cards: vec![
                    RewardCard::new(CardId::PommelStrike, 0),
                    RewardCard::new(CardId::ShrugItOff, 0),
                ],
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let mut env = LearningEnvV3::from_session(session);

        for expected in ["gold", "potion"] {
            let boundary = env.observe().expect("reward learning boundary");
            let LearningBoundaryV1::Strategic {
                boundary: strategic_boundary,
            } = &boundary
            else {
                panic!("reward boundary should remain strategic");
            };
            let decision =
                LearningModelDecisionV1::from_boundary(&boundary).expect("reward decision");
            assert_eq!(decision.candidates.len(), 1);
            let exposed_id = strategic_learning_policy_candidates(strategic_boundary)[0]
                .candidate_id
                .as_str();
            assert_eq!(
                decision.strategic_ordinal_for_planner_candidate_id(exposed_id),
                Some(0)
            );
            let filtered_id = strategic_boundary
                .legal_candidates
                .candidates
                .iter()
                .find(|candidate| candidate.candidate_id != exposed_id)
                .map(|candidate| candidate.candidate_id.as_str())
                .expect("reward fixture should retain a filtered candidate");
            assert_eq!(
                decision.strategic_ordinal_for_planner_candidate_id(filtered_id),
                None
            );
            let LearningModelCandidateSemanticsV1::Strategic { action } =
                decision.candidates[0].semantics
            else {
                panic!("reward decision should remain strategic");
            };
            match (
                expected,
                action.semantics().expect("public reward semantics"),
            ) {
                (
                    "gold",
                    LearningStrategicActionSemanticsV2::ClaimReward {
                        reward:
                            crate::ai::planner_core::PlannerRewardDescriptor::Gold { amount: 12 },
                        ..
                    },
                )
                | (
                    "potion",
                    LearningStrategicActionSemanticsV2::ClaimReward {
                        reward:
                            crate::ai::planner_core::PlannerRewardDescriptor::Potion {
                                potion: PotionId::SmokeBomb,
                            },
                        ..
                    },
                ) => {}
                _ => panic!("unexpected canonical reward action: {action:?}"),
            }
            let action = match decision.choose(0).expect("choose forced reward action") {
                LearningModelChoiceV1::Apply(action) => action,
                LearningModelChoiceV1::DecodeSelection(_) => {
                    panic!("reward action should not start a selection")
                }
            };
            let prepared = env
                .prepare_action(action)
                .expect("prepare forced reward action");
            env.step_prepared(prepared)
                .expect("apply forced reward action");
        }

        let boundary = env.observe().expect("reward ready to reveal");
        let decision = LearningModelDecisionV1::from_boundary(&boundary).unwrap();
        assert!(
            decision.candidates.len() > 1,
            "revealing must not remove navigation"
        );
        let open = decision
            .candidates
            .iter()
            .position(|candidate| match candidate.semantics {
                LearningModelCandidateSemanticsV1::Strategic { action } => matches!(
                    action.semantics(),
                    Ok(LearningStrategicActionSemanticsV2::OpenCardReward { .. })
                ),
                _ => false,
            })
            .unwrap();
        let LearningModelChoiceV1::Apply(action) = decision.choose(open).unwrap() else {
            panic!("reveal is atomic")
        };
        let prepared = env.prepare_action(action).unwrap();
        env.step_prepared(prepared).unwrap();

        let boundary = env.observe().expect("opened card reward boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("card choice decision");
        assert!(decision.candidates.len() >= 3);
        assert!(decision
            .candidates
            .iter()
            .any(|candidate| match candidate.semantics {
                LearningModelCandidateSemanticsV1::Strategic { action } => matches!(
                    action.semantics(),
                    Ok(LearningStrategicActionSemanticsV2::TakeCard {
                        card: CardId::PommelStrike,
                        ..
                    })
                ),
                _ => false,
            }));
        assert!(decision
            .candidates
            .iter()
            .any(|candidate| match candidate.semantics {
                LearningModelCandidateSemanticsV1::Strategic { action } => matches!(
                    action.semantics(),
                    Ok(LearningStrategicActionSemanticsV2::SkipCardReward { .. })
                ),
                _ => false,
            }));
        let skip = decision
            .candidates
            .iter()
            .position(|candidate| match candidate.semantics {
                LearningModelCandidateSemanticsV1::Strategic { action } => matches!(
                    action.semantics(),
                    Ok(LearningStrategicActionSemanticsV2::SkipCardReward { .. })
                ),
                _ => false,
            })
            .unwrap();
        let LearningModelChoiceV1::Apply(action) = decision.choose(skip).unwrap() else {
            panic!("skip is atomic")
        };
        let prepared = env.prepare_action(action).unwrap();
        env.step_prepared(prepared).unwrap();
        let after_skip = env.observe().unwrap();
        let decision = LearningModelDecisionV1::from_boundary(&after_skip).unwrap();
        assert!(
            decision
                .candidates
                .iter()
                .any(|candidate| match candidate.semantics {
                    LearningModelCandidateSemanticsV1::Strategic { action } => matches!(
                        action.semantics(),
                        Ok(LearningStrategicActionSemanticsV2::Proceed { .. })
                    ),
                    _ => false,
                }),
            "a skipped reward must leave an executable exit"
        );
    }

    #[test]
    fn strategic_reward_policy_keeps_multiple_potions_and_card_rewards_as_choices() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Potion {
                potion_id: PotionId::FruitJuice,
            },
            RewardItem::Potion {
                potion_id: PotionId::FearPotion,
            },
            RewardItem::Potion {
                potion_id: PotionId::StrengthPotion,
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("multi-potion reward boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("multi-potion decision");
        assert_eq!(
            decision
                .candidates
                .iter()
                .filter(|candidate| match candidate.semantics {
                    LearningModelCandidateSemanticsV1::Strategic { action } => matches!(
                        action.semantics(),
                        Ok(LearningStrategicActionSemanticsV2::ClaimReward {
                            reward: crate::ai::planner_core::PlannerRewardDescriptor::Potion { .. },
                            ..
                        })
                    ),
                    _ => false,
                })
                .count(),
            3
        );

        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::PommelStrike, 0)],
            },
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::ShrugItOff, 0)],
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("multi-card-reward boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("multi-card-reward decision");
        assert_eq!(
            decision
                .candidates
                .iter()
                .filter(|candidate| match candidate.semantics {
                    LearningModelCandidateSemanticsV1::Strategic { action } => matches!(
                        action.semantics(),
                        Ok(LearningStrategicActionSemanticsV2::OpenCardReward { .. })
                    ),
                    _ => false,
                })
                .count(),
            2
        );
    }

    #[test]
    fn ragged_batch_owns_row_splits_and_builds_padding_only_on_request() {
        let first_env = LearningEnvV3::new(RunControlConfig::default());
        let first = first_env.observe().expect("first strategic boundary");
        let mut second_session = RunControlSession::new(RunControlConfig::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Bash, 51)];
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        combat.entities.monsters.push(monster);
        second_session.engine_state = EngineState::CombatPlayerTurn;
        second_session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let second = LearningEnvV3::from_session(second_session)
            .observe()
            .expect("combat boundary");
        let boundaries = vec![first, second];
        let batch = LearningModelBatchV1::from_boundaries(&boundaries).expect("build ragged batch");

        assert_eq!(batch.candidate_row_splits.len(), 3);
        assert_eq!(
            batch.flattened_candidate_count(),
            batch
                .decisions
                .iter()
                .map(|row| row.candidates.len())
                .sum::<usize>()
        );
        let mask = batch.dense_action_mask();
        assert_eq!(mask.rows, 2);
        for (row, decision) in batch.decisions.iter().enumerate() {
            for column in 0..mask.width {
                assert_eq!(
                    mask.values[row * mask.width + column],
                    column < decision.candidates.len()
                );
            }
        }
    }

    #[test]
    fn combat_model_views_use_local_monster_indices_and_hide_runtime_entity_ids() {
        const PRIVATE_ENTITY_ID: usize = 3_000_000_001;
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Bash, 51)];
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = PRIVATE_ENTITY_ID;
        combat.entities.monsters.push(monster);
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("targeted combat boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("build model decision");

        assert!(decision.candidates.iter().any(|candidate| matches!(
            candidate.semantics,
            LearningModelCandidateSemanticsV1::CombatAtomic {
                action: LearningCombatAtomicActionV1::PlayCard {
                    hand_index: 0,
                    target_monster_index: Some(0),
                },
            }
        )));
        assert!(matches!(
            decision.observation,
            LearningModelObservationV1::Combat(LearningCombatModelObservationV1 {
                monsters,
                ..
            }) if monsters.len() == 1 && monsters.get(0).is_some()
        ));
        let rendered = format!("{decision:?}");
        assert!(
            !rendered.contains(&PRIVATE_ENTITY_ID.to_string()),
            "model-facing debug output leaked a private monster entity id: {rendered}"
        );
    }

    #[test]
    fn indexed_atomic_choice_carries_typed_semantics() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let choice = PendingChoice::DiscoverySelect(crate::state::DiscoveryChoiceState {
            cards: vec![CardId::Bash, CardId::FiendFire],
            colorless: false,
            card_type: None,
            amount: 1,
            can_skip: true,
        });
        session.engine_state = EngineState::PendingChoice(choice.clone());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::PendingChoice(choice),
            crate::test_support::blank_test_combat(),
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("indexed boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("build model decision");

        assert!(decision.candidates.iter().any(|candidate| {
            let LearningModelCandidateSemanticsV1::CombatAtomic {
                action:
                    LearningCombatAtomicActionV1::SubmitIndexedChoice {
                        choice_index: 1,
                        indexed,
                    },
            } = candidate.semantics
            else {
                return false;
            };
            indexed.input_encoding == CombatIndexedChoiceInputEncodingV2::SubmitDiscoverChoiceIndex
                && matches!(
                    indexed.reason,
                    PublicCombatIndexedChoiceReasonV1::Discovery {
                        colorless: false,
                        card_type: None,
                        amount: 1,
                    }
                )
                && matches!(
                    indexed.candidate,
                    PublicCombatIndexedChoiceCandidateV1::Card {
                        card_id: CardId::FiendFire,
                        upgrades: 0,
                    }
                )
        }));
    }

    #[test]
    fn symbolic_scry_decodes_linearly_and_submits_only_after_explicit_stop() {
        let mut combat = crate::test_support::blank_test_combat();
        let card_lineages = crate::test_support::scenario_lineages(1..=64);
        combat.zones.draw_pile = (card_lineages
            .iter()
            .copied()
            .map(|lineage| CombatCard::with_lineage(CardId::Strike, lineage))
            .collect::<Vec<_>>())
        .into();
        let projection = crate::agent::information::action::project_public_combat_actions_v1(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards: vec![CardId::Strike; 64],
                card_lineages,
            }),
            &combat,
        )
        .unwrap();
        let mut draft = LearningSelectionDraftV1::from_combat(
            projection.public.selection_families[0].clone(),
            projection.private_resolution.selection_families[0].clone(),
        );

        let first = draft.decision();
        assert_eq!(first.candidates.len(), 65);
        assert!(matches!(
            first.candidates[0].semantics,
            LearningSelectionCandidateSemanticsV1::Submit
        ));
        assert!(matches!(
            draft.choose(1).expect("append first card"),
            LearningSelectionStepV1::Continue
        ));
        assert_eq!(draft.selected_domain_indices(), &[0]);
        assert_eq!(draft.decision().candidates.len(), 64);
        assert_eq!(
            match draft.choose(0).expect("submit prefix") {
                LearningSelectionStepV1::Apply(action) => action,
                LearningSelectionStepV1::Continue => panic!("submit must produce an action"),
            },
            LearningActionV1::CombatInput {
                input: ClientInput::SubmitScryDiscard(vec![0]),
            }
        );
    }

    #[test]
    fn inverse_scry_path_round_trips_empty_and_ordered_inputs_without_changing_source() {
        let (_, draft) =
            public_scry_selection_fixture(crate::test_support::scenario_lineages([11, 22, 33]));
        for indices in [vec![], vec![2, 0]] {
            let input = ClientInput::SubmitScryDiscard(indices);
            let (_, action) = draft.ordinal_path_for_input_v1(&input).unwrap().unwrap();
            assert_eq!(action, LearningActionV1::CombatInput { input });
        }
        for indices in [vec![1, 1], vec![3]] {
            assert!(draft
                .ordinal_path_for_input_v1(&ClientInput::SubmitScryDiscard(indices))
                .unwrap()
                .is_none());
        }
        assert!(draft.selected_domain_indices().is_empty());
    }

    fn public_scry_selection_fixture(
        card_lineages: Vec<CardLineageId>,
    ) -> (LearningBoundaryV1, LearningSelectionDraftV1) {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.draw_pile = (card_lineages
            .iter()
            .copied()
            .map(|lineage| CombatCard::with_lineage(CardId::Strike, lineage))
            .collect::<Vec<_>>())
        .into();
        let choice = PendingChoice::ScrySelect {
            cards: vec![CardId::Strike; card_lineages.len()],
            card_lineages,
        };
        session.engine_state = EngineState::PendingChoice(choice.clone());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::PendingChoice(choice),
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("scry selection boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("scry root decision");
        let family_ordinal = decision
            .candidates
            .iter()
            .position(|candidate| {
                matches!(
                    candidate.semantics,
                    LearningModelCandidateSemanticsV1::CombatSelectionFamily { .. }
                )
            })
            .expect("scry selection family candidate");
        let LearningModelChoiceV1::DecodeSelection(draft) = decision
            .choose(family_ordinal)
            .expect("start public scry decoder")
        else {
            panic!("scry root must start a symbolic decoder");
        };
        (boundary, draft)
    }

    fn append_selection_domain(draft: &mut LearningSelectionDraftV1, domain_index: usize) {
        let candidate_ordinal = draft
            .decision()
            .candidates
            .iter()
            .position(|candidate| {
                matches!(
                    candidate.semantics,
                    LearningSelectionCandidateSemanticsV1::Append {
                        domain_index: candidate_domain
                    } if candidate_domain == domain_index
                )
            })
            .expect("append candidate for public selection domain");
        assert!(matches!(
            draft
                .choose(candidate_ordinal)
                .expect("append public selection domain"),
            LearningSelectionStepV1::Continue
        ));
    }

    #[test]
    fn public_selection_snapshot_binds_the_symbolic_prefix_and_candidate_surface() {
        let (boundary, mut draft) =
            public_scry_selection_fixture(crate::test_support::scenario_lineages([11, 22, 33]));
        let before = super::super::learning_public_selection_snapshot_v1(
            &boundary,
            &CombatLearningPotionPolicyV1::All,
            &draft,
        )
        .expect("empty-prefix public snapshot");

        append_selection_domain(&mut draft, 0);
        let after = super::super::learning_public_selection_snapshot_v1(
            &boundary,
            &CombatLearningPotionPolicyV1::All,
            &draft,
        )
        .expect("one-item-prefix public snapshot");

        assert_ne!(before.snapshot_id, after.snapshot_id);
        assert_ne!(before.observation, after.observation);
        assert_ne!(before.history_snapshot, after.history_snapshot);
        assert_ne!(before.candidate_surface, after.candidate_surface);
        assert_eq!(
            after.candidate_surface.ordered_candidate_ids.len(),
            draft.decision().candidates.len()
        );
    }

    #[test]
    fn public_selection_snapshot_ignores_private_card_lineages() {
        fn snapshot(
            card_lineages: Vec<CardLineageId>,
        ) -> crate::ai::planner_core::PublicInformationSnapshotV1 {
            let (boundary, mut draft) = public_scry_selection_fixture(card_lineages);
            append_selection_domain(&mut draft, 0);
            super::super::learning_public_selection_snapshot_v1(
                &boundary,
                &CombatLearningPotionPolicyV1::All,
                &draft,
            )
            .expect("public selection snapshot")
        }

        let first = snapshot(crate::test_support::scenario_lineages([11, 22, 33]));
        let same_public_state = snapshot(crate::test_support::scenario_lineages([101, 202, 303]));

        assert_eq!(first.observation, same_public_state.observation);
        assert_eq!(first.history_snapshot, same_public_state.history_snapshot);
        assert_eq!(first.candidate_surface, same_public_state.candidate_surface);
        assert_eq!(first.snapshot_id, same_public_state.snapshot_id);
    }

    #[test]
    fn symbolic_run_card_selection_applies_only_after_explicit_submit() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Defend, 22),
            CombatCard::for_scenario(CardId::Bash, 33),
        ];
        session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 2,
            max_choices: 2,
            reason: RunPendingChoiceReason::Transform,
            source: DomainEventSource::Selection(SelectionReason::Transform),
            return_state: Box::new(EngineState::MapNavigation),
        });
        let mut env = LearningEnvV3::from_session(session);
        let boundary = env.observe().expect("run selection boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("root model decision");
        let LearningModelChoiceV1::DecodeSelection(mut draft) =
            decision.choose(0).expect("start run selection")
        else {
            panic!("run selection root must start a symbolic decoder");
        };

        assert!(draft.run_family().is_some());
        assert!(matches!(
            draft.choose(0).expect("append first card"),
            LearningSelectionStepV1::Continue
        ));
        assert!(matches!(
            draft.choose(0).expect("append second card"),
            LearningSelectionStepV1::Continue
        ));
        let LearningSelectionStepV1::Apply(action) = draft.choose(0).expect("submit selection")
        else {
            panic!("explicit submit must produce a run action");
        };
        let LearningActionV1::RunSelection { resolution, .. } = &action else {
            panic!("run decoder must produce a run selection action");
        };
        assert_eq!(resolution.scope, SelectionScope::Deck);
        assert_eq!(
            resolution.selected_card_lineages(),
            crate::test_support::scenario_lineages([11, 22])
        );

        let prepared = env
            .prepare_action(action)
            .expect("prepare typed run selection");
        let step = env
            .step_prepared(prepared)
            .expect("apply typed run selection");
        assert!(matches!(
            step.boundary,
            LearningBoundaryV1::Strategic { .. }
        ));
    }

    #[test]
    fn symbolic_model_views_and_debug_output_hide_runtime_card_lineages() {
        let private_lineage = crate::test_support::scenario_lineage(3_000_001);
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.draw_pile =
            (vec![CombatCard::with_lineage(CardId::Strike, private_lineage)]).into();
        let projection = crate::agent::information::action::project_public_combat_actions_v1(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards: vec![CardId::Strike],
                card_lineages: vec![private_lineage],
            }),
            &combat,
        )
        .unwrap();
        let draft = LearningSelectionDraftV1::from_combat(
            projection.public.selection_families[0].clone(),
            projection.private_resolution.selection_families[0].clone(),
        );
        let family = draft.combat_family().expect("combat family");
        assert_eq!(
            family.domain(0).expect("public domain").semantics(),
            LearningCombatSelectionDomainSemanticsV1::Scry {
                index: 0,
                card_id: Some(CardId::Strike),
                currently_present: true,
            }
        );
        for rendered in [
            format!("{family:?}"),
            format!("{:?}", family.domain(0).expect("public domain")),
            format!("{draft:?}"),
            format!("{:?}", draft.decision()),
        ] {
            assert!(
                !rendered.contains(&private_lineage.to_string()),
                "model-facing debug output leaked a private card lineage: {rendered}"
            );
        }
    }

    #[test]
    fn duplicate_scry_uuid_is_removed_from_the_decoder_after_one_address_is_chosen() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 7)]).into();
        let projection = crate::agent::information::action::project_public_combat_actions_v1(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards: vec![CardId::Strike, CardId::Strike],
                card_lineages: crate::test_support::scenario_lineages([7, 7]),
            }),
            &combat,
        )
        .unwrap();
        let mut draft = LearningSelectionDraftV1::from_combat(
            projection.public.selection_families[0].clone(),
            projection.private_resolution.selection_families[0].clone(),
        );

        assert_eq!(draft.decision().candidates.len(), 3);
        assert!(matches!(
            draft.choose(1).expect("choose first address"),
            LearningSelectionStepV1::Continue
        ));
        assert_eq!(draft.decision().candidates.len(), 1);
        assert!(matches!(
            draft.decision().candidates[0].semantics,
            LearningSelectionCandidateSemanticsV1::Submit
        ));
    }

    #[test]
    fn symbolic_decoder_rows_batch_with_their_parent_observations() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            crate::test_support::blank_test_combat(),
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let observation = LearningModelDecisionV1::from_boundary(&boundary)
            .expect("model decision")
            .observation;

        let mut selection_combat = crate::test_support::blank_test_combat();
        selection_combat.zones.draw_pile = ((1..=3)
            .map(|uuid| CombatCard::for_scenario(CardId::Strike, uuid))
            .collect::<Vec<_>>())
        .into();
        let projection = crate::agent::information::action::project_public_combat_actions_v1(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards: vec![CardId::Strike; 3],
                card_lineages: crate::test_support::scenario_lineages([1, 2, 3]),
            }),
            &selection_combat,
        )
        .unwrap();
        let first = LearningSelectionDraftV1::from_combat(
            projection.public.selection_families[0].clone(),
            projection.private_resolution.selection_families[0].clone(),
        );
        let mut second = first.clone();
        assert!(matches!(
            second.choose(1).expect("append first domain item"),
            LearningSelectionStepV1::Continue
        ));

        let batch = LearningSelectionModelBatchV1::from_rows([
            (observation, &first),
            (observation, &second),
        ])
        .expect("batch selection decisions");
        assert_eq!(batch.candidate_row_splits, vec![0, 4, 7]);
        assert_eq!(batch.flattened_candidate_count(), 7);
        assert_eq!(
            batch.dense_action_mask(),
            LearningDenseActionMaskV1 {
                rows: 2,
                width: 4,
                values: vec![true, true, true, true, true, true, true, false],
            }
        );
        assert!(matches!(
            batch.rows[0].observation,
            LearningModelObservationV1::Combat(_)
        ));
    }

    #[test]
    fn symbolic_hand_selection_resolves_from_root_ordinal_to_one_exact_input() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Defend, 12),
        ];
        let choice = PendingChoice::HandSelect {
            candidate_lineages: crate::test_support::scenario_lineages([11, 12]),
            min_cards: 1,
            max_cards: 2,
            can_cancel: false,
            reason: HandSelectReason::Discard,
        };
        session.engine_state = EngineState::PendingChoice(choice.clone());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::PendingChoice(choice),
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("hand-selection boundary");
        let decision =
            LearningModelDecisionV1::from_boundary(&boundary).expect("build root decision");
        assert_eq!(decision.candidates.len(), 1);
        let LearningModelChoiceV1::DecodeSelection(mut draft) =
            decision.choose(0).expect("choose symbolic family")
        else {
            panic!("symbolic family must start a draft");
        };

        assert_eq!(draft.decision().candidates.len(), 2);
        assert!(matches!(
            draft.choose(0).expect("append first hand card"),
            LearningSelectionStepV1::Continue
        ));
        assert_eq!(
            match draft.choose(0).expect("submit selected card") {
                LearningSelectionStepV1::Apply(action) => action,
                LearningSelectionStepV1::Continue => panic!("submit must produce an action"),
            },
            LearningActionV1::CombatInput {
                input: ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                    SelectionScope::Hand,
                    [crate::test_support::scenario_lineage(11)],
                )),
            }
        );
    }

    #[test]
    fn incomplete_strategic_surface_is_rejected_before_inference() {
        let env = LearningEnvV3::new(RunControlConfig::default());
        let LearningBoundaryV1::Strategic { mut boundary } =
            env.observe().expect("initial strategic boundary")
        else {
            panic!("expected strategic boundary");
        };
        boundary.legal_candidates.completeness =
            crate::ai::planner_core::CandidateSetCompleteness::Incomplete {
                basis: crate::ai::planner_core::CandidateCompletenessBasis::RunControlBoundaryEnumerator,
                gaps: vec![
                    crate::ai::planner_core::CandidateRepresentationGap::UnsupportedBoundaryAction,
                ],
            };

        assert_eq!(
            LearningModelDecisionV1::from_boundary(&LearningBoundaryV1::Strategic { boundary })
                .expect_err("incomplete surface must not reach inference"),
            LearningModelInputError::IncompleteStrategicCandidateSet {
                site: crate::ai::planner_core::PlannerDecisionSite::Neow,
                gaps: vec![
                    crate::ai::planner_core::CandidateRepresentationGap::UnsupportedBoundaryAction,
                ],
            }
        );
    }

    #[test]
    fn malformed_indexed_surface_is_rejected_before_inference() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            crate::test_support::blank_test_combat(),
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let LearningBoundaryV1::Combat { mut boundary } = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary")
        else {
            panic!("expected combat boundary");
        };
        boundary.public_actions.atomic_actions =
            vec![PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index: 0 }];
        boundary.private_resolution.atomic_inputs = vec![ClientInput::SubmitDiscoverChoice(0)];

        assert_eq!(
            LearningModelDecisionV1::from_boundary(&LearningBoundaryV1::Combat { boundary })
                .expect_err("missing typed indexed metadata must fail"),
            LearningModelInputError::IndexedChoiceMetadataMissing
        );
    }

    #[test]
    fn disabled_symbolic_family_is_not_a_model_candidate() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 7)]).into();
        let surface = combat_legal_action_surface_v2(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards: vec![CardId::Strike, CardId::Defend],
                card_lineages: crate::test_support::scenario_lineages([7]),
            }),
            &combat,
        );
        assert_eq!(
            surface.selection_families[0].selection_status,
            crate::sim::combat_action_surface::CombatSelectionStatusV2::Disabled(
                crate::sim::combat_action_surface::CombatSelectionDisabledReasonV2::MalformedScryDomain
            )
        );
        let projection = crate::agent::information::action::project_public_combat_actions_v1(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards: vec![CardId::Strike, CardId::Defend],
                card_lineages: crate::test_support::scenario_lineages([7]),
            }),
            &combat,
        )
        .expect("project disabled exact family");
        let boundary = LearningBoundaryV1::Combat {
            boundary: LearningCombatBoundaryV1 {
                observation: crate::agent::information::state::public_combat_state_v1(&combat),
                public_run_context: PublicCombatRunContextV1::Unavailable {
                    reason: crate::agent::information::run::PublicCombatRunContextGapV1::DetachedExactCombatPosition,
                },
                observation_completeness: super::super::LearningObservationCompletenessV1::Complete,
                public_actions: projection.public,
                private_resolution: projection.private_resolution,
            },
        };
        assert_eq!(
            LearningModelDecisionV1::from_boundary(&boundary)
                .expect_err("disabled-only action surface must not reach a model"),
            LearningModelInputError::NoLegalCandidates
        );
    }
}
