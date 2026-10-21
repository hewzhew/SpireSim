//! Content-addressed public decision snapshots over model-facing inputs.
//!
//! This typed projector is separate from the inference view so model batching
//! stays serialization-free. Simulator-private card lineages, potion UUIDs,
//! and entity ids remain in the
//! decision resolution table and never enter public snapshot identity.

use serde::Serialize;

use crate::agent::information::action::{
    PublicCombatIndexedChoiceCandidateV1, PublicCombatIndexedChoiceReasonV1,
};
use crate::agent::information::run::PublicCombatRunContextV1;
use crate::ai::planner_core::{
    stable_planner_id, CandidateSetCompleteness, PlannerDecisionContext, PlannerDecisionSite,
    PlannerMechanicsManifest, PlannerPublicHistory, PlannerPublicMap, PlannerRelicObservation,
    PlannerRunGoal, PlannerRunScalars, PublicCandidateSurfaceKindV1,
    PublicCandidateSurfaceReferenceV1, PublicDecisionDomainV1, PublicHistorySnapshotReferenceV1,
    PublicInformationSnapshotV1, PublicObservationReferenceV1, PublicObservationScopeV1,
    PLANNER_MECHANICS_ID, PLANNER_MECHANICS_VERSION,
};
use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::sim::combat_action_surface::{
    CombatIndexedChoiceInputEncodingV2, CombatSelectionInputEncodingV2,
    CombatSelectionPayloadLanguageV2, CombatSelectionReasonV2,
};
use crate::state::core::PileType;
use crate::state::selection::{SelectionReason, SelectionScope};

use super::learning_model_input::{
    CombatLearningPotionPolicyV1, LearningCombatAtomicActionV1, LearningCombatModelObservationV1,
    LearningCombatMonsterV1, LearningCombatSelectionDomainSemanticsV1,
    LearningCombatSelectionFamilyV1, LearningModelCandidateSemanticsV1, LearningModelCandidateV1,
    LearningModelDecisionV1, LearningModelObservationV1, LearningRunSelectionFamilyV1,
    LearningSelectionCandidateSemanticsV1, LearningSelectionDraftV1,
    LearningStrategicActionSemanticsV2, LearningStrategicModelObservationV1,
    LearningStrategicPotionSlotV1, LearningStrategicPotionV1,
};
use super::LearningBoundaryV1;

pub const LEARNING_PUBLIC_STRATEGIC_OBSERVATION_SCHEMA_NAME: &str =
    "LearningPublicStrategicObservationV1";
pub const LEARNING_PUBLIC_STRATEGIC_OBSERVATION_SCHEMA_VERSION: u32 = 1;
pub const LEARNING_PUBLIC_COMBAT_OBSERVATION_SCHEMA_NAME: &str =
    "LearningPublicCombatObservationV1";
pub const LEARNING_PUBLIC_COMBAT_OBSERVATION_SCHEMA_VERSION: u32 = 1;
pub const LEARNING_PUBLIC_SELECTION_OBSERVATION_SCHEMA_NAME: &str =
    "LearningPublicSelectionObservationV1";
pub const LEARNING_PUBLIC_SELECTION_OBSERVATION_SCHEMA_VERSION: u32 = 1;
pub const LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_NAME: &str = "LearningPublicCandidateSurfaceV1";
pub const LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_VERSION: u32 = 1;
pub const LEARNING_PUBLIC_STRATEGIC_HISTORY_SNAPSHOT_SCHEMA_NAME: &str =
    "LearningPublicStrategicHistorySnapshotV1";
pub const LEARNING_PUBLIC_COMBAT_HISTORY_SNAPSHOT_SCHEMA_NAME: &str =
    "LearningPublicCombatHistorySnapshotV1";
pub const LEARNING_PUBLIC_SELECTION_HISTORY_SNAPSHOT_SCHEMA_NAME: &str =
    "LearningPublicSelectionHistorySnapshotV1";
pub const LEARNING_PUBLIC_HISTORY_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

impl LearningModelDecisionV1<'_> {
    /// Stable public identities aligned to the model-facing policy surface.
    /// The observation-local ordinal distinguishes separate executable
    /// candidates whose visible mechanics are intentionally equivalent.
    pub fn public_candidate_ids_v1(
        &self,
        public_observation_id: &str,
    ) -> Result<Vec<String>, String> {
        if public_observation_id.trim().is_empty() {
            return Err("public observation id must not be empty".into());
        }
        self.candidates
            .iter()
            .enumerate()
            .map(|(ordinal, candidate)| {
                let payload = public_candidate_identity_payload_v1(&self.observation, candidate)?;
                stable_planner_id(
                    "learning_public_candidate_v1",
                    &(public_observation_id, ordinal, payload),
                )
            })
            .collect()
    }
}

/// Captures one sanitized observation and its complete deployable policy
/// surface. It deliberately does not claim a normalized trajectory prefix,
/// chance ensemble, search receipt, or training-label authority.
pub fn learning_public_information_snapshot_v1(
    boundary: &LearningBoundaryV1,
) -> Result<PublicInformationSnapshotV1, String> {
    learning_public_information_snapshot_with_potion_policy_v1(
        boundary,
        &CombatLearningPotionPolicyV1::All,
    )
}

pub fn learning_public_information_snapshot_with_potion_policy_v1(
    boundary: &LearningBoundaryV1,
    potion_policy: &CombatLearningPotionPolicyV1,
) -> Result<PublicInformationSnapshotV1, String> {
    learning_public_information_snapshot_with_surface_v1(
        boundary,
        potion_policy,
        super::LearningCandidateSurfaceV1::Policy,
    )
}

pub fn learning_public_information_snapshot_with_surface_v1(
    boundary: &LearningBoundaryV1,
    potion_policy: &CombatLearningPotionPolicyV1,
    surface: super::LearningCandidateSurfaceV1,
) -> Result<PublicInformationSnapshotV1, String> {
    let model_decision =
        LearningModelDecisionV1::from_boundary_with_surface(boundary, potion_policy, surface)
            .map_err(|error| error.to_string())?;
    learning_public_root_snapshot_from_model_v1(boundary, &model_decision)
}

pub fn learning_public_selection_snapshot_v1(
    boundary: &LearningBoundaryV1,
    potion_policy: &CombatLearningPotionPolicyV1,
    draft: &LearningSelectionDraftV1,
) -> Result<PublicInformationSnapshotV1, String> {
    learning_public_selection_snapshot_with_surface_v1(
        boundary,
        potion_policy,
        super::LearningCandidateSurfaceV1::Policy,
        draft,
    )
}

pub fn learning_public_selection_snapshot_with_surface_v1(
    boundary: &LearningBoundaryV1,
    potion_policy: &CombatLearningPotionPolicyV1,
    surface: super::LearningCandidateSurfaceV1,
    draft: &LearningSelectionDraftV1,
) -> Result<PublicInformationSnapshotV1, String> {
    let model_decision =
        LearningModelDecisionV1::from_boundary_with_surface(boundary, potion_policy, surface)
            .map_err(|error| error.to_string())?;
    let parent = learning_public_root_snapshot_from_model_v1(boundary, &model_decision)?;
    let family = public_selection_family_identity_v1(model_decision.observation, draft)?;
    let selected_domain_indices = draft.selected_domain_indices();
    let observation = PublicObservationReferenceV1::from_sanitized_payload(
        LEARNING_PUBLIC_SELECTION_OBSERVATION_SCHEMA_NAME,
        LEARNING_PUBLIC_SELECTION_OBSERVATION_SCHEMA_VERSION,
        parent.observation.scope,
        &PublicSelectionObservationIdentityV1 {
            parent_observation_id: &parent.observation.observation_id,
            family,
            selected_domain_indices,
        },
    )?;
    let decision = draft.decision();
    let candidate_ids = decision
        .candidates
        .iter()
        .enumerate()
        .map(|(ordinal, candidate)| {
            stable_planner_id(
                "learning_public_selection_candidate_v1",
                &(
                    observation.observation_id.as_str(),
                    ordinal,
                    PublicSelectionCandidateIdentityV1::from(candidate.semantics),
                ),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let candidate_surface = PublicCandidateSurfaceReferenceV1::from_candidate_ids(
        LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_NAME,
        LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_VERSION,
        PublicCandidateSurfaceKindV1::DeployablePolicy,
        candidate_ids,
    )?;
    let history_snapshot = PublicHistorySnapshotReferenceV1::from_sanitized_payload(
        LEARNING_PUBLIC_SELECTION_HISTORY_SNAPSHOT_SCHEMA_NAME,
        LEARNING_PUBLIC_HISTORY_SNAPSHOT_SCHEMA_VERSION,
        &PublicSelectionHistorySnapshotIdentityV1 {
            parent_history_snapshot_id: &parent.history_snapshot.history_snapshot_id,
            selected_domain_indices,
        },
    )?;
    PublicInformationSnapshotV1::new(
        parent.domain,
        observation,
        history_snapshot,
        candidate_surface,
    )
}

fn learning_public_root_snapshot_from_model_v1(
    boundary: &LearningBoundaryV1,
    model_decision: &LearningModelDecisionV1<'_>,
) -> Result<PublicInformationSnapshotV1, String> {
    match (boundary, model_decision.observation) {
        (
            LearningBoundaryV1::Strategic { boundary },
            LearningModelObservationV1::Strategic(model_observation),
        ) => {
            if !matches!(
                boundary.legal_candidates.completeness,
                CandidateSetCompleteness::Complete { .. }
            ) {
                return Err("public strategic snapshot requires complete candidate capture".into());
            }
            let observation = PublicObservationReferenceV1::from_sanitized_payload(
                LEARNING_PUBLIC_STRATEGIC_OBSERVATION_SCHEMA_NAME,
                LEARNING_PUBLIC_STRATEGIC_OBSERVATION_SCHEMA_VERSION,
                PublicObservationScopeV1::StrategicRunDecision,
                &PublicStrategicObservationIdentityV1::from_model(
                    &boundary.observation.mechanics,
                    model_observation,
                ),
            )?;
            let candidate_surface = PublicCandidateSurfaceReferenceV1::from_candidate_ids(
                LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_NAME,
                LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_VERSION,
                PublicCandidateSurfaceKindV1::DeployablePolicy,
                model_decision.public_candidate_ids_v1(&observation.observation_id)?,
            )?;
            let history_snapshot = PublicHistorySnapshotReferenceV1::from_sanitized_payload(
                LEARNING_PUBLIC_STRATEGIC_HISTORY_SNAPSHOT_SCHEMA_NAME,
                LEARNING_PUBLIC_HISTORY_SNAPSHOT_SCHEMA_VERSION,
                model_observation.public_history,
            )?;
            PublicInformationSnapshotV1::new(
                PublicDecisionDomainV1::Strategic,
                observation,
                history_snapshot,
                candidate_surface,
            )
        }
        (
            LearningBoundaryV1::Combat { boundary },
            LearningModelObservationV1::Combat(model_observation),
        ) => {
            let scope = if boundary.public_run_context.is_available() {
                PublicObservationScopeV1::CombatDecisionWithRunContinuation
            } else {
                PublicObservationScopeV1::CombatDecisionOnly
            };
            let observation = PublicObservationReferenceV1::from_sanitized_payload(
                LEARNING_PUBLIC_COMBAT_OBSERVATION_SCHEMA_NAME,
                LEARNING_PUBLIC_COMBAT_OBSERVATION_SCHEMA_VERSION,
                scope,
                &PublicCombatObservationIdentityV1::from_model(model_observation),
            )?;
            let candidate_surface = PublicCandidateSurfaceReferenceV1::from_candidate_ids(
                LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_NAME,
                LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_VERSION,
                PublicCandidateSurfaceKindV1::DeployablePolicy,
                model_decision.public_candidate_ids_v1(&observation.observation_id)?,
            )?;
            let history_snapshot = PublicHistorySnapshotReferenceV1::from_sanitized_payload(
                LEARNING_PUBLIC_COMBAT_HISTORY_SNAPSHOT_SCHEMA_NAME,
                LEARNING_PUBLIC_HISTORY_SNAPSHOT_SCHEMA_VERSION,
                &CombatPublicHistorySnapshotV1::from_model(model_observation),
            )?;
            PublicInformationSnapshotV1::new(
                PublicDecisionDomainV1::Combat,
                observation,
                history_snapshot,
                candidate_surface,
            )
        }
        (LearningBoundaryV1::Terminal { .. }, _) => {
            Err("terminal boundary has no deployable policy candidate surface".into())
        }
        (LearningBoundaryV1::Unsupported, _) => {
            Err("unsupported boundary has no public information snapshot".into())
        }
        _ => Err("learning boundary and model observation domains do not match".into()),
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicSelectionObservationIdentityV1<'a> {
    parent_observation_id: &'a str,
    family: PublicSelectionFamilyIdentityV1,
    selected_domain_indices: &'a [usize],
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicSelectionHistorySnapshotIdentityV1<'a> {
    parent_history_snapshot_id: &'a str,
    selected_domain_indices: &'a [usize],
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PublicSelectionFamilyIdentityV1 {
    Combat {
        family: PublicCombatSelectionFamilyIdentityV1,
    },
    Run {
        family: PublicRunSelectionFamilyIdentityV1,
    },
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicRunSelectionFamilyIdentityV1 {
    scope: SelectionScope,
    reason: SelectionReason,
    declared_min: usize,
    declared_max: usize,
    effective_max: usize,
    selectable_card_ordinals: Vec<usize>,
}

fn public_selection_family_identity_v1(
    observation: LearningModelObservationV1<'_>,
    draft: &LearningSelectionDraftV1,
) -> Result<PublicSelectionFamilyIdentityV1, String> {
    match (observation, draft.combat_family(), draft.run_family()) {
        (LearningModelObservationV1::Combat(_), Some(family), None) => {
            Ok(PublicSelectionFamilyIdentityV1::Combat {
                family: public_combat_selection_family_identity_v1(family),
            })
        }
        (LearningModelObservationV1::Strategic(_), None, Some(family)) => {
            Ok(PublicSelectionFamilyIdentityV1::Run {
                family: public_run_selection_family_identity_v1(family)?,
            })
        }
        _ => Err("selection family does not match its public observation domain".into()),
    }
}

fn public_run_selection_family_identity_v1(
    family: LearningRunSelectionFamilyV1<'_>,
) -> Result<PublicRunSelectionFamilyIdentityV1, String> {
    let selectable_card_ordinals = (0..family.domain_count())
        .map(|index| {
            family
                .domain_card_ordinal(index)
                .ok_or_else(|| "run selection domain is not aligned".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PublicRunSelectionFamilyIdentityV1 {
        scope: family.scope(),
        reason: family.reason(),
        declared_min: family.declared_min(),
        declared_max: family.declared_max(),
        effective_max: family.effective_max(),
        selectable_card_ordinals,
    })
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PublicSelectionCandidateIdentityV1 {
    Submit,
    Append { domain_index: usize },
}

impl From<LearningSelectionCandidateSemanticsV1> for PublicSelectionCandidateIdentityV1 {
    fn from(value: LearningSelectionCandidateSemanticsV1) -> Self {
        match value {
            LearningSelectionCandidateSemanticsV1::Submit => Self::Submit,
            LearningSelectionCandidateSemanticsV1::Append { domain_index } => {
                Self::Append { domain_index }
            }
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicStrategicObservationIdentityV1<'a> {
    mechanics: &'a PlannerMechanicsManifest,
    run_goal: PlannerRunGoal,
    decision_site: PlannerDecisionSite,
    run: &'a PlannerRunScalars,
    cards: Vec<PublicStrategicCardIdentityV1>,
    relics: &'a [PlannerRelicObservation],
    potions: Vec<PublicStrategicPotionSlotIdentityV1>,
    public_map: &'a PlannerPublicMap,
    context: &'a PlannerDecisionContext,
    public_history: &'a PlannerPublicHistory,
}

impl<'a> PublicStrategicObservationIdentityV1<'a> {
    fn from_model(
        mechanics: &'a PlannerMechanicsManifest,
        observation: LearningStrategicModelObservationV1<'a>,
    ) -> Self {
        Self {
            mechanics,
            run_goal: observation.run_goal,
            decision_site: observation.decision_site,
            run: observation.run,
            cards: observation
                .cards()
                .map(PublicStrategicCardIdentityV1::from)
                .collect(),
            relics: observation.relics,
            potions: observation
                .potion_slots()
                .map(PublicStrategicPotionSlotIdentityV1::from)
                .collect(),
            public_map: observation.public_map,
            context: observation.context,
            public_history: observation.public_history,
        }
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(deny_unknown_fields)]
struct PublicStrategicCardIdentityV1 {
    card: CardId,
    upgrades: u8,
    misc_value: i32,
    base_damage_override: Option<i32>,
    base_block_override: Option<i32>,
    cost_modifier: i8,
}

impl From<super::learning_model_input::LearningStrategicCardV2<'_>>
    for PublicStrategicCardIdentityV1
{
    fn from(card: super::learning_model_input::LearningStrategicCardV2<'_>) -> Self {
        Self {
            card: card.card(),
            upgrades: card.upgrades(),
            misc_value: card.misc_value(),
            base_damage_override: card.base_damage_override(),
            base_block_override: card.base_block_override(),
            cost_modifier: card.cost_modifier(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicStrategicPotionSlotIdentityV1 {
    slot: usize,
    potion: Option<PublicStrategicPotionIdentityV1>,
}

impl From<LearningStrategicPotionSlotV1<'_>> for PublicStrategicPotionSlotIdentityV1 {
    fn from(slot: LearningStrategicPotionSlotV1<'_>) -> Self {
        Self {
            slot: slot.slot(),
            potion: slot.potion().map(PublicStrategicPotionIdentityV1::from),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicStrategicPotionIdentityV1 {
    potion: PotionId,
    can_use: bool,
    can_discard: bool,
    requires_target: bool,
}

impl From<LearningStrategicPotionV1<'_>> for PublicStrategicPotionIdentityV1 {
    fn from(potion: LearningStrategicPotionV1<'_>) -> Self {
        Self {
            potion: potion.potion(),
            can_use: potion.can_use(),
            can_discard: potion.can_discard(),
            requires_target: potion.requires_target(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicCombatObservationIdentityV1<'a> {
    mechanics_id: &'static str,
    mechanics_version: u32,
    public_run_context: &'a PublicCombatRunContextV1,
    potions: Vec<Option<PublicCombatPotionIdentityV1>>,
    hidden_reasons: &'a [crate::agent::information::combat::HiddenInformationReasonV1],
    encounter: &'a crate::agent::information::state::CombatLearningEncounterV1,
    turn: &'a crate::agent::information::state::CombatLearningTurnV1,
    player: &'a crate::agent::information::state::CombatLearningPlayerStateV1,
    cards: &'a crate::agent::information::state::CombatLearningCardZonesV1,
    monsters: Vec<PublicCombatMonsterIdentityV1<'a>>,
}

impl<'a> PublicCombatObservationIdentityV1<'a> {
    fn from_model(observation: LearningCombatModelObservationV1<'a>) -> Self {
        Self {
            mechanics_id: PLANNER_MECHANICS_ID,
            mechanics_version: PLANNER_MECHANICS_VERSION,
            public_run_context: observation.public_run_context,
            potions: observation
                .potions
                .iter()
                .map(|slot| slot.as_ref().map(PublicCombatPotionIdentityV1::from))
                .collect(),
            hidden_reasons: observation.hidden_reasons,
            encounter: observation.encounter,
            turn: observation.turn,
            player: observation.player,
            cards: observation.cards,
            monsters: (0..observation.monsters.len())
                .filter_map(|index| observation.monsters.get(index))
                .map(PublicCombatMonsterIdentityV1::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicCombatPotionIdentityV1 {
    potion_id: PotionId,
    can_use: bool,
    can_discard: bool,
    requires_target: bool,
}

impl From<&crate::agent::information::state::CombatLearningPotionV1>
    for PublicCombatPotionIdentityV1
{
    fn from(potion: &crate::agent::information::state::CombatLearningPotionV1) -> Self {
        Self {
            potion_id: potion.potion_id,
            can_use: potion.can_use,
            can_discard: potion.can_discard,
            requires_target: potion.requires_target,
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicCombatMonsterIdentityV1<'a> {
    slot: u8,
    enemy: crate::agent::information::state::CombatLearningEnemyIdentityV1,
    hp: i32,
    max_hp: i32,
    block: i32,
    alive: bool,
    escaped: bool,
    dying: bool,
    half_dead: bool,
    intent: &'a crate::agent::information::state::CombatLearningIntentV1,
    executed_moves: &'a crate::agent::information::state::CombatLearningMonsterMoveHistoryV1,
    public_counters: &'a [crate::agent::information::state::CombatLearningMonsterPublicCounterV1],
    powers: &'a [crate::agent::information::state::CombatLearningPowerV1],
}

impl<'a> From<LearningCombatMonsterV1<'a>> for PublicCombatMonsterIdentityV1<'a> {
    fn from(monster: LearningCombatMonsterV1<'a>) -> Self {
        Self {
            slot: monster.slot(),
            enemy: monster.enemy(),
            hp: monster.hp(),
            max_hp: monster.max_hp(),
            block: monster.block(),
            alive: monster.alive(),
            escaped: monster.escaped(),
            dying: monster.dying(),
            half_dead: monster.half_dead(),
            intent: monster.intent(),
            executed_moves: monster.executed_moves(),
            public_counters: monster.public_counters(),
            powers: monster.powers(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CombatPublicHistorySnapshotV1<'a> {
    turn_count: u32,
    turn_counters: &'a crate::agent::information::state::CombatLearningTurnCountersV1,
    monster_histories: Vec<CombatPublicMonsterHistorySnapshotV1<'a>>,
}

impl<'a> CombatPublicHistorySnapshotV1<'a> {
    fn from_model(observation: LearningCombatModelObservationV1<'a>) -> Self {
        Self {
            turn_count: observation.turn.turn_count,
            turn_counters: &observation.turn.counters,
            monster_histories: (0..observation.monsters.len())
                .filter_map(|index| observation.monsters.get(index))
                .map(|monster| CombatPublicMonsterHistorySnapshotV1 {
                    slot: monster.slot(),
                    executed_moves: monster.executed_moves(),
                    public_counters: monster.public_counters(),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CombatPublicMonsterHistorySnapshotV1<'a> {
    slot: u8,
    executed_moves: &'a crate::agent::information::state::CombatLearningMonsterMoveHistoryV1,
    public_counters: &'a [crate::agent::information::state::CombatLearningMonsterPublicCounterV1],
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PublicCandidateIdentityPayloadV1 {
    Strategic {
        action: LearningStrategicActionSemanticsV2,
    },
    CombatAtomic {
        action: PublicCombatAtomicIdentityV1,
    },
    CombatSelectionFamily {
        family: PublicCombatSelectionFamilyIdentityV1,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PublicCombatAtomicIdentityV1 {
    PlayCard {
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
        input_encoding: CombatIndexedChoiceInputEncodingV2,
        reason: PublicCombatIndexedChoiceReasonV1,
        candidate: PublicCombatIndexedChoiceCandidateV1,
    },
    Proceed,
    Cancel,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PublicCombatSelectionFamilyIdentityV1 {
    input_encoding: CombatSelectionInputEncodingV2,
    reason: CombatSelectionReasonV2,
    source_pile: Option<PileType>,
    raw_domain_count: u64,
    eligible_domain_count: u64,
    max_distinct_selection_count: u64,
    declared_min: u64,
    declared_max: u64,
    effective_max: u64,
    payload_language: CombatSelectionPayloadLanguageV2,
    domain: Vec<PublicCombatSelectionDomainIdentityV1>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PublicCombatSelectionDomainIdentityV1 {
    Card {
        ordinal: u64,
        card_id: Option<CardId>,
        upgrades: Option<u8>,
        eligible: bool,
    },
    Scry {
        index: u64,
        card_id: Option<CardId>,
        currently_present: bool,
    },
}

impl From<LearningCombatSelectionDomainSemanticsV1> for PublicCombatSelectionDomainIdentityV1 {
    fn from(value: LearningCombatSelectionDomainSemanticsV1) -> Self {
        match value {
            LearningCombatSelectionDomainSemanticsV1::Card {
                ordinal,
                card_id,
                upgrades,
                eligible,
            } => Self::Card {
                ordinal,
                card_id,
                upgrades,
                eligible,
            },
            LearningCombatSelectionDomainSemanticsV1::Scry {
                index,
                card_id,
                currently_present,
            } => Self::Scry {
                index,
                card_id,
                currently_present,
            },
        }
    }
}

fn public_combat_selection_family_identity_v1(
    family: LearningCombatSelectionFamilyV1<'_>,
) -> PublicCombatSelectionFamilyIdentityV1 {
    let domain = (0..family.domain_count())
        .filter_map(|index| family.domain(index))
        .map(|candidate| candidate.semantics().into())
        .collect();
    PublicCombatSelectionFamilyIdentityV1 {
        input_encoding: family.input_encoding(),
        reason: family.reason().clone(),
        source_pile: family.source_pile(),
        raw_domain_count: family.raw_domain_count(),
        eligible_domain_count: family.eligible_domain_count(),
        max_distinct_selection_count: family.max_distinct_selection_count(),
        declared_min: family.declared_min(),
        declared_max: family.declared_max(),
        effective_max: family.effective_max(),
        payload_language: family.payload_language(),
        domain,
    }
}

fn public_candidate_identity_payload_v1(
    observation: &LearningModelObservationV1<'_>,
    candidate: &LearningModelCandidateV1<'_>,
) -> Result<PublicCandidateIdentityPayloadV1, String> {
    match (*observation, candidate.semantics) {
        (
            LearningModelObservationV1::Strategic(_),
            LearningModelCandidateSemanticsV1::Strategic { action },
        ) => Ok(PublicCandidateIdentityPayloadV1::Strategic {
            action: action.semantics().map_err(|error| error.to_string())?,
        }),
        (
            LearningModelObservationV1::Combat(_),
            LearningModelCandidateSemanticsV1::CombatAtomic { action },
        ) => {
            let action = match action {
                LearningCombatAtomicActionV1::PlayCard {
                    target_monster_index,
                    ..
                } => PublicCombatAtomicIdentityV1::PlayCard {
                    target_monster_index,
                },
                LearningCombatAtomicActionV1::UsePotion {
                    potion_index,
                    target_monster_index,
                } => PublicCombatAtomicIdentityV1::UsePotion {
                    potion_index,
                    target_monster_index,
                },
                LearningCombatAtomicActionV1::DiscardPotion { potion_index } => {
                    PublicCombatAtomicIdentityV1::DiscardPotion { potion_index }
                }
                LearningCombatAtomicActionV1::EndTurn => PublicCombatAtomicIdentityV1::EndTurn,
                LearningCombatAtomicActionV1::SubmitIndexedChoice {
                    choice_index,
                    indexed,
                } => PublicCombatAtomicIdentityV1::SubmitIndexedChoice {
                    choice_index,
                    input_encoding: indexed.input_encoding,
                    reason: indexed.reason.clone(),
                    candidate: *indexed.candidate,
                },
                LearningCombatAtomicActionV1::Proceed => PublicCombatAtomicIdentityV1::Proceed,
                LearningCombatAtomicActionV1::Cancel => PublicCombatAtomicIdentityV1::Cancel,
            };
            Ok(PublicCandidateIdentityPayloadV1::CombatAtomic { action })
        }
        (
            LearningModelObservationV1::Combat(_),
            LearningModelCandidateSemanticsV1::CombatSelectionFamily { family },
        ) => Ok(PublicCandidateIdentityPayloadV1::CombatSelectionFamily {
            family: public_combat_selection_family_identity_v1(family),
        }),
        _ => Err("model observation and candidate semantics domains do not match".into()),
    }
}
