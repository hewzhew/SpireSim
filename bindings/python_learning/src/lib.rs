#[cfg(test)]
compile_error!(
    "sts_learning_bridge has no Rust lib-test harness; run the owning workspace crate tests or bindings/python_learning/verify.ps1"
);

use std::collections::BTreeSet;

use numpy::ndarray::Array2;
use numpy::{IntoPyArray, PyArray1};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use sts_complete_turn_env::{
    replay_verified_winning_complete_turn_line_v1, CombatWitnessMaterializedOutcomeV1,
    CompleteTurnExactEnvV1, CompleteTurnExactStepV1,
};
use sts_complete_turn_proposal::ExactCompleteTurnProposalBoundaryV1;
use sts_oracle_eval::sim::combat::CombatTerminal;
use sts_oracle_learning::ai::planner_core::{PublicDecisionDomainV1, PublicInformationSnapshotV1};
use sts_oracle_learning::eval::run_control::{
    combat_entry_floor_chance_population_v1, combat_public_chance_particle_checkpoints_v1,
    public_deterministic_baseline_after_reward_proceed_v1,
    public_deterministic_baseline_decision_v1, public_deterministic_baseline_manifest_v1,
    public_deterministic_baseline_selection_decision_v1, CombatLearningPotionPolicyV1,
    CombatLearningRootBatchArtifactV1, CombatLearningRootV1, CompleteTurnCandidateEvidenceV1,
    CompleteTurnSelectedActionV1, CompleteTurnWinningDemonstrationV1, FixedCombatBranchBatchV1,
    FixedCombatBranchChoiceV1, FixedCombatOptionsV1, FixedCombatResearchV1, FixedCombatStepV1,
    LearningActionV1, LearningBoundaryKindV1, LearningBoundaryV1, LearningCandidateSurfaceV1,
    LearningDecisionPathV3, LearningDecisionTokenV3, LearningEnvPoolV3, LearningEnvV3,
    LearningExecutionInputsV1, LearningModelDecisionV1, LearningPublicRunContextV1,
    LearningSelectionDraftV1, LearningTransitionV3, PublicBaselineCoverageV1,
    PublicBaselineInformationAccessV1, RunControlConfig, RunControlSessionCheckpointV1,
    PUBLIC_DETERMINISTIC_BASELINE_NAME_V1, PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
};
use sts_oracle_learning_projection::privileged_node_sl_oracle_path_from_decision_v1;

mod bridge_decision;
mod checkpoint;
mod combat_batch;
mod distance;
mod guided;
mod policy;
mod segment;
mod semantic;

use bridge_decision::{
    bridge_states_ready, choose_bridge_ordinals, collect_ready_actions,
    combat_decision_audit_json_from_source, decision_snapshot_from_source,
    public_information_snapshots_from_learning_source, semantic_snapshot_from_source,
    states_from_source, strategic_decision_audit_json_from_source,
    tokenized_decision_snapshot_from_source, LearningBatchDecisionSource,
};
use checkpoint::{LearningCheckpointBatch, LearningSlotCheckpoint};
use combat_batch::{
    potion_id_names, CombatLearningBatchEnv, CombatLearningDecisionProgressV1,
    CombatLearningRecoveryRoot, PyCombatLearningRootContextV1, COMBAT_TERMINAL_LOSS,
    COMBAT_TERMINAL_UNRESOLVED, COMBAT_TERMINAL_WIN,
};

use semantic::{
    ActionKind, CardZoneKind, CategoricalField, CombatActionKind, CompleteTurnBoundaryKind,
    CompleteTurnCandidateEvidenceKind, CompleteTurnMilestoneKind, ContextKind, CounterItemKind,
    IndexedChoiceCandidateKind, IndexedChoiceReasonKind, IntentKind, MapRoomKind,
    PublicCounterKind, QuestionMarkOutcomeKind, RelationKind, RewardKind, ScalarField,
    SelectionCandidateKind, SelectionDomainKind, SelectionReasonKind, SemanticBatch,
    SemanticBatchBuilder, SemanticCompleteness, ShopPurchaseStatusKind, TokenKind,
    CARD_ID_VOCABULARY_SIZE, CARD_MECHANIC_COVERAGE_SCHEMA, CARD_MECHANIC_ROLE_SCHEMA,
    CARD_RARITY_SCHEMA, CARD_TAG_SCHEMA, CARD_TARGET_SCHEMA, CARD_TYPE_SCHEMA,
    CATEGORICAL_VOCABULARY_SIZES, ENCOUNTER_ID_VOCABULARY_SIZE, ENEMY_ID_VOCABULARY_SIZE,
    EVENT_ID_VOCABULARY_SIZE, IDENTITY_RESIDUAL_CATEGORICAL_FIELDS, NO_CANDIDATE_TOKEN,
    POTION_CLASS_SCHEMA, POTION_ID_VOCABULARY_SIZE, POTION_MECHANIC_ROLE_SCHEMA,
    POTION_RARITY_SCHEMA, POWER_AMOUNT_KIND_SCHEMA, POWER_EFFECT_SCHEMA,
    POWER_EFFECT_TARGET_SCHEMA, POWER_ID_VOCABULARY_SIZE, POWER_LIFETIME_SCHEMA,
    POWER_TRIGGER_SCHEMA, RELIC_ID_VOCABULARY_SIZE, SEMANTIC_SCHEMA_VERSION,
};

fn parse_candidate_surface(value: &str) -> PyResult<LearningCandidateSurfaceV1> {
    match value {
        "policy" => Ok(LearningCandidateSurfaceV1::Policy),
        "legal" => Ok(LearningCandidateSurfaceV1::Legal),
        _ => Err(value_error("candidate_surface must be 'policy' or 'legal'")),
    }
}

fn parse_player_class(value: &str) -> PyResult<&'static str> {
    match value {
        "ironclad" => Ok("Ironclad"),
        "silent" => Ok("Silent"),
        "defect" => Ok("Defect"),
        "watcher" => Ok("Watcher"),
        _ => Err(value_error(
            "player_class must be ironclad, silent, defect, or watcher",
        )),
    }
}

fn parse_neow_eligibility(
    value: &str,
) -> PyResult<sts_oracle_learning::content::events::neow::NeowEligibilityV1> {
    use sts_oracle_learning::content::events::neow::NeowEligibilityV1;
    match value {
        "rich" => Ok(NeowEligibilityV1::Rich),
        "mini" => Ok(NeowEligibilityV1::Mini),
        _ => Err(value_error("neow_eligibility must be rich or mini")),
    }
}

const PHASE_STRATEGIC_ROOT: u8 = 0;
const PHASE_COMBAT_ROOT: u8 = 1;
const PHASE_SELECTION: u8 = 2;
const RUN_BOUNDARY_STRATEGIC: u8 = 0;
const RUN_BOUNDARY_COMBAT: u8 = 1;
const RUN_BOUNDARY_TERMINAL: u8 = 2;
const RUN_BOUNDARY_UNSUPPORTED: u8 = 3;
const MAX_EXPORTED_COMBAT_ROOTS: usize = 64;
const MAX_COMBAT_ENTRY_FLOOR_CHANCE_CANDIDATES: usize = 1_000_000;

#[derive(Clone, Debug)]
enum BridgeSlotState {
    Terminal,
    Root,
    Selection {
        draft: LearningSelectionDraftV1,
        decision_ordinals: Vec<usize>,
        source_token: Option<LearningDecisionTokenV3>,
    },
    Ready {
        action: LearningActionV1,
        decision_ordinals: Vec<usize>,
        source_token: Option<LearningDecisionTokenV3>,
    },
}

impl BridgeSlotState {
    fn decision_ordinals(&self) -> &[usize] {
        match self {
            Self::Terminal | Self::Root => &[],
            Self::Selection {
                decision_ordinals, ..
            }
            | Self::Ready {
                decision_ordinals, ..
            } => decision_ordinals,
        }
    }
}

#[derive(Debug)]
struct DecisionSnapshot {
    slot_indices: Vec<usize>,
    phases: Vec<u8>,
    candidate_counts: Vec<usize>,
    candidate_row_splits: Vec<usize>,
    decision_token_ids: Option<Vec<String>>,
    public_snapshot_ids: Option<Vec<String>>,
    decision_steps: Option<Vec<u64>>,
}

#[derive(Debug)]
struct LegalityBootstrapBehaviorSnapshot {
    available: Vec<bool>,
    ordinals: Vec<usize>,
}

#[derive(Debug)]
struct PrivilegedNodeSLOracleSnapshot {
    available: Vec<bool>,
    ordinal_paths: Vec<Vec<usize>>,
}

#[pyclass(frozen, name = "LearningPublicRunContextV1")]
struct PyLearningPublicRunContextV1 {
    inner: LearningPublicRunContextV1,
}

#[pyclass(frozen, name = "LearningPublicDecisionSnapshotV1")]
struct PyLearningPublicDecisionSnapshotV1 {
    phase: u8,
    inner: PublicInformationSnapshotV1,
}

#[pymethods]
impl PyLearningPublicDecisionSnapshotV1 {
    #[getter]
    fn phase(&self) -> u8 {
        self.phase
    }

    #[getter]
    fn is_combat(&self) -> bool {
        self.inner.domain == PublicDecisionDomainV1::Combat
    }

    #[getter]
    fn snapshot_id(&self) -> String {
        self.inner.snapshot_id.clone()
    }

    #[getter]
    fn observation_id(&self) -> String {
        self.inner.observation.observation_id.clone()
    }

    #[getter]
    fn history_snapshot_id(&self) -> String {
        self.inner.history_snapshot.history_snapshot_id.clone()
    }

    #[getter]
    fn candidate_surface_id(&self) -> String {
        self.inner.candidate_surface.candidate_surface_id.clone()
    }

    #[getter]
    fn candidate_ids(&self) -> Vec<String> {
        self.inner.candidate_surface.ordered_candidate_ids.clone()
    }
}

/// Compact, on-demand audit facts for one exact combat root.
///
/// This view is deliberately separate from the per-decision semantic batch:
/// collection and training journals can inspect curriculum composition without
/// copying the deck and relic inventory through every model inference call.
#[pyclass(frozen, name = "CombatLearningRootAuditV1")]
struct PyCombatLearningRootAuditV1 {
    seed: u64,
    act: u8,
    floor: i32,
    ascension_level: u8,
    hp: i32,
    max_hp: i32,
    potion_ids: Vec<Option<String>>,
    encounter_id: String,
    monster_ids: Vec<String>,
    is_elite_fight: bool,
    is_boss_fight: bool,
    master_deck_cards: Vec<(String, u8)>,
    relic_ids: Vec<String>,
}

#[pymethods]
impl PyCombatLearningRootAuditV1 {
    #[getter]
    fn seed(&self) -> u64 {
        self.seed
    }

    #[getter]
    fn act(&self) -> u8 {
        self.act
    }

    #[getter]
    fn floor(&self) -> i32 {
        self.floor
    }

    #[getter]
    fn ascension_level(&self) -> u8 {
        self.ascension_level
    }

    #[getter]
    fn hp(&self) -> i32 {
        self.hp
    }

    #[getter]
    fn max_hp(&self) -> i32 {
        self.max_hp
    }

    #[getter]
    fn potion_ids(&self) -> Vec<Option<String>> {
        self.potion_ids.clone()
    }

    #[getter]
    fn encounter_id(&self) -> String {
        self.encounter_id.clone()
    }

    #[getter]
    fn monster_ids(&self) -> Vec<String> {
        self.monster_ids.clone()
    }

    #[getter]
    fn is_elite_fight(&self) -> bool {
        self.is_elite_fight
    }

    #[getter]
    fn is_boss_fight(&self) -> bool {
        self.is_boss_fight
    }

    #[getter]
    fn master_deck_cards(&self) -> Vec<(String, u8)> {
        self.master_deck_cards.clone()
    }

    #[getter]
    fn relic_ids(&self) -> Vec<String> {
        self.relic_ids.clone()
    }
}

#[pymethods]
impl PyLearningPublicRunContextV1 {
    #[getter]
    fn boundary_kind(&self) -> u8 {
        match self.inner.boundary_kind {
            LearningBoundaryKindV1::Strategic => RUN_BOUNDARY_STRATEGIC,
            LearningBoundaryKindV1::Combat => RUN_BOUNDARY_COMBAT,
            LearningBoundaryKindV1::Terminal => RUN_BOUNDARY_TERMINAL,
            LearningBoundaryKindV1::Unsupported => RUN_BOUNDARY_UNSUPPORTED,
        }
    }

    #[getter]
    fn strategic_context_kind(&self) -> Option<u8> {
        self.inner.strategic_context_kind.map(|kind| kind as u8)
    }

    #[getter]
    fn is_combat(&self) -> bool {
        self.inner.boundary_kind == LearningBoundaryKindV1::Combat
    }

    #[getter]
    fn is_terminal(&self) -> bool {
        self.inner.boundary_kind == LearningBoundaryKindV1::Terminal
    }

    #[getter]
    fn seed(&self) -> u64 {
        self.inner.seed
    }

    #[getter]
    fn ascension_level(&self) -> u8 {
        self.inner.ascension_level
    }

    #[getter]
    fn act(&self) -> u8 {
        self.inner.act
    }

    #[getter]
    fn floor(&self) -> i32 {
        self.inner.floor
    }

    #[getter]
    fn hp(&self) -> i32 {
        self.inner.hp
    }

    #[getter]
    fn max_hp(&self) -> i32 {
        self.inner.max_hp
    }

    #[getter]
    fn gold(&self) -> i32 {
        self.inner.gold
    }

    #[getter]
    fn potion_ids(&self) -> Vec<Option<String>> {
        potion_id_names(&self.inner.potion_ids)
    }

    #[getter]
    fn encounter_id(&self) -> Option<String> {
        self.inner
            .encounter_id
            .map(|encounter| encounter.stable_id().to_string())
    }

    #[getter]
    fn monster_ids(&self) -> Vec<String> {
        self.inner
            .monster_ids
            .iter()
            .map(|monster| monster.stable_id().to_string())
            .collect()
    }
}

/// Opaque witness; continued research environments cannot mutate its source line.
#[pyclass(frozen)]
struct FixedCombatBranchChoice {
    choice: FixedCombatBranchChoiceV1,
}

#[pymethods]
impl FixedCombatBranchChoice {
    #[getter]
    fn source_slot_index(&self) -> usize {
        self.choice.slot_index()
    }

    #[getter]
    fn source_decision_token_id(&self) -> &str {
        &self.choice.source().token_id
    }

    #[getter]
    fn candidate_ordinal(&self) -> usize {
        self.choice.candidate_ordinal()
    }
}

/// Resident source-bound Oracle research; construction belongs to the source env.
#[pyclass]
struct FixedCombatResearch {
    research: FixedCombatResearchV1,
}

#[pymethods]
impl FixedCombatResearch {
    /// Cumulative work target, capped by the creation contract. Reuses all
    /// previous search and its remaining total wall allowance.
    fn advance_to<'py>(
        &mut self,
        py: Python<'py>,
        generation_work: usize,
    ) -> PyResult<Bound<'py, PyDict>> {
        let branches = py
            .detach(|| self.research.advance_to(generation_work))
            .map_err(runtime_error)?;
        fixed_combat_branches_to_python(py, branches, self.research.slot_index())
    }
}

fn fixed_combat_branches_to_python<'py>(
    py: Python<'py>,
    branches: FixedCombatBranchBatchV1,
    slot_index: usize,
) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("information_access", "oracle_exact_combat")?;
    result.set_item("source_slot_index", slot_index)?;
    result.set_item("source_decision_token_id", branches.source.token_id)?;
    result.set_item(
        "source_public_snapshot_id",
        branches.source.public_snapshot_id,
    )?;
    result.set_item("source_decision_step", branches.source.decision_step)?;
    result.set_item(
        "entry_json",
        serde_json::to_string(&branches.entry).map_err(runtime_error)?,
    )?;
    result.set_item(
        "options_json",
        serde_json::to_string(&branches.options).map_err(runtime_error)?,
    )?;
    result.set_item(
        "attempt_json",
        serde_json::to_string(&branches.attempt).map_err(runtime_error)?,
    )?;
    result.set_item("candidate_ordinals", branches.candidate_ordinals)?;
    result.set_item("elapsed_ms", branches.elapsed_ms)?;
    let environments = PyList::empty(py);
    for pool in branches.environments {
        let states = states_from_source(&pool).map_err(runtime_error)?;
        environments.append(Py::new(py, LearningBatchEnv { pool, states })?)?;
    }
    result.set_item("environments", environments)?;
    let choices = PyList::empty(py);
    for choice in branches.choices {
        choices.append(Py::new(py, FixedCombatBranchChoice { choice })?)?;
    }
    result.set_item("choices", choices)?;
    Ok(result)
}

#[pyclass]
struct LearningBatchEnv {
    pool: LearningEnvPoolV3,
    states: Vec<BridgeSlotState>,
}

#[pymethods]
impl LearningBatchEnv {
    #[new]
    #[pyo3(signature = (seeds, ascension_level, *, final_act=false, candidate_surface="policy", player_class="ironclad", neow_eligibility="rich"))]
    fn new(
        seeds: Vec<u64>,
        ascension_level: u8,
        final_act: bool,
        candidate_surface: &str,
        player_class: &str,
        neow_eligibility: &str,
    ) -> PyResult<Self> {
        Self::from_seeds_with_potion_policy(
            seeds,
            ascension_level,
            CombatLearningPotionPolicyV1::All,
            final_act,
            parse_candidate_surface(candidate_surface)?,
            parse_player_class(player_class)?,
            parse_neow_eligibility(neow_eligibility)?,
        )
    }

    #[staticmethod]
    #[pyo3(signature = (seeds, ascension_level, *, final_act=false, candidate_surface="policy", player_class="ironclad", neow_eligibility="rich"))]
    fn without_combat_potions(
        seeds: Vec<u64>,
        ascension_level: u8,
        final_act: bool,
        candidate_surface: &str,
        player_class: &str,
        neow_eligibility: &str,
    ) -> PyResult<Self> {
        Self::from_seeds_with_potion_policy(
            seeds,
            ascension_level,
            CombatLearningPotionPolicyV1::never(),
            final_act,
            parse_candidate_surface(candidate_surface)?,
            parse_player_class(player_class)?,
            parse_neow_eligibility(neow_eligibility)?,
        )
    }

    #[getter]
    fn candidate_surface(&self) -> &'static str {
        match self.pool.candidate_surface() {
            LearningCandidateSurfaceV1::Policy => "policy",
            LearningCandidateSurfaceV1::Legal => "legal",
        }
    }

    /// Explicit constructed first-act boss campfires, not natural checkpoints.
    #[staticmethod]
    fn constructed_campfires(recipes_json: &str) -> PyResult<Self> {
        if recipes_json.len() > 1_048_576 {
            return Err(value_error(
                "constructed campfire recipes exceed byte bound",
            ));
        }
        let recipes: Vec<sts_oracle_learning::eval::run_control::ConstructedFirstActLoadoutV1> =
            serde_json::from_str(recipes_json).map_err(value_error)?;
        if recipes.is_empty() || recipes.len() > 128 {
            return Err(value_error(
                "constructed campfire batch must contain 1..128 recipes",
            ));
        }
        let envs = recipes
            .iter()
            .map(sts_oracle_learning::eval::run_control::constructed_campfire_env_v1)
            .collect::<Result<Vec<_>, _>>()
            .map_err(value_error)?;
        let pool = LearningEnvPoolV3::from_envs(envs).map_err(runtime_error)?;
        let states = states_from_source(&pool).map_err(runtime_error)?;
        Ok(Self { pool, states })
    }

    /// Explicit first-act card choices with a declared synthetic corridor.
    #[staticmethod]
    fn constructed_card_rewards(recipes_json: &str) -> PyResult<Self> {
        if recipes_json.len() > 1_048_576 {
            return Err(value_error("constructed reward recipes exceed byte bound"));
        }
        let recipes: Vec<sts_oracle_learning::eval::run_control::ConstructedCardRewardRecipeV1> =
            serde_json::from_str(recipes_json).map_err(value_error)?;
        if recipes.is_empty() || recipes.len() > 128 {
            return Err(value_error(
                "constructed reward batch must contain 1..128 recipes",
            ));
        }
        let envs = recipes
            .iter()
            .map(sts_oracle_learning::eval::run_control::constructed_card_reward_env_v1)
            .collect::<Result<Vec<_>, _>>()
            .map_err(value_error)?;
        let pool = LearningEnvPoolV3::from_envs(envs).map_err(runtime_error)?;
        let states = states_from_source(&pool).map_err(runtime_error)?;
        Ok(Self { pool, states })
    }

    #[getter]
    fn slot_count(&self) -> usize {
        self.pool.slot_count()
    }

    #[getter]
    fn terminal_count(&self) -> usize {
        self.pool.terminal_count()
    }

    #[getter]
    fn ready(&self) -> bool {
        bridge_states_ready(&self.states)
    }

    #[pyo3(signature = (*, max_bytes))]
    fn checkpoint_bytes<'py>(
        &self,
        py: Python<'py>,
        max_bytes: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let payload = self
            .encode_cross_process_checkpoint(max_bytes)
            .map_err(value_error)?;
        Ok(PyBytes::new(py, &payload))
    }

    #[staticmethod]
    #[pyo3(signature = (payload, *, expected_slots, max_bytes))]
    fn from_checkpoint_bytes(
        payload: &[u8],
        expected_slots: usize,
        max_bytes: usize,
    ) -> PyResult<Self> {
        Self::decode_cross_process_checkpoint(payload, expected_slots, max_bytes)
            .map_err(value_error)
    }

    /// Construct a fresh batch from exact production combat-root artifacts.
    ///
    /// Rust decodes and validates every opaque run-control checkpoint before
    /// exposing the first slot. Python never receives simulator session data.
    #[staticmethod]
    #[pyo3(signature = (payload, *, expected_roots, max_bytes, allow_combat_potions=true))]
    fn from_combat_root_artifact_bytes(
        payload: &[u8],
        expected_roots: usize,
        max_bytes: usize,
        allow_combat_potions: bool,
    ) -> PyResult<Self> {
        let policy = if allow_combat_potions {
            CombatLearningPotionPolicyV1::All
        } else {
            CombatLearningPotionPolicyV1::never()
        };
        Self::decode_combat_root_artifact(payload, expected_roots, max_bytes, policy)
            .map_err(value_error)
    }

    /// Construct an independent-stream feasibility population from one opaque combat root.
    ///
    /// Every returned slot has the same public observation and legal candidates as the source,
    /// but independently sampled hidden draw order and future combat RNG. Particle seeds are
    /// caller-owned provenance and are never exposed to the model-facing decision batch.
    /// ``draw_order_permutation`` is an optional, validated offline coupling of
    /// sampled draw positions, not an observation or a run-seed posterior.
    #[staticmethod]
    #[pyo3(signature = (payload, *, expected_roots, source_slot, particle_seeds, max_bytes, draw_order_permutation=None))]
    fn from_combat_public_chance_particles(
        payload: &[u8],
        expected_roots: usize,
        source_slot: usize,
        particle_seeds: Vec<u64>,
        max_bytes: usize,
        draw_order_permutation: Option<Vec<usize>>,
    ) -> PyResult<Self> {
        if particle_seeds.len() > MAX_EXPORTED_COMBAT_ROOTS {
            return Err(PyValueError::new_err(format!(
                "combat public-chance population exceeds {MAX_EXPORTED_COMBAT_ROOTS} particles"
            )));
        }
        let artifact =
            CombatLearningRootBatchArtifactV1::decode(payload, expected_roots, max_bytes)
                .map_err(value_error)?;
        let checkpoints = artifact.into_checkpoints().map_err(value_error)?;
        let source = checkpoints.get(source_slot).cloned().ok_or_else(|| {
            PyValueError::new_err(format!(
                "combat public-chance source slot {source_slot} is out of range"
            ))
        })?;
        let particles = combat_public_chance_particle_checkpoints_v1(
            source,
            &particle_seeds,
            draw_order_permutation.as_deref(),
        )
        .map_err(value_error)?;
        Self::from_combat_root_checkpoints(particles, CombatLearningPotionPolicyV1::All)
            .map_err(value_error)
    }

    /// Reconstruct a public-conditioned combat-entry population from floor-local seed bases.
    ///
    /// Rust scans candidate floor seed bases through the production combat-start constructor and
    /// retains only complete public-decision matches. Persistent RNG streams stay conditioned on
    /// the exact upstream run. The returned seed list is private provenance and is not included in
    /// model inputs. This first surface intentionally accepts only turn-0 room combats with visible
    /// intent and an empty potion inventory.
    #[staticmethod]
    #[pyo3(signature = (payload, *, expected_roots, source_slot, candidate_floor_seed_base_start, max_candidates, required_particles, max_bytes))]
    fn from_combat_entry_floor_chance_particles(
        payload: &[u8],
        expected_roots: usize,
        source_slot: usize,
        candidate_floor_seed_base_start: u64,
        max_candidates: usize,
        required_particles: usize,
        max_bytes: usize,
    ) -> PyResult<(Self, Vec<u64>, usize, usize, usize)> {
        if required_particles == 0 || required_particles > MAX_EXPORTED_COMBAT_ROOTS {
            return Err(PyValueError::new_err(format!(
                "combat-entry floor chance population requires 1..={MAX_EXPORTED_COMBAT_ROOTS} particles"
            )));
        }
        if max_candidates == 0 || max_candidates > MAX_COMBAT_ENTRY_FLOOR_CHANCE_CANDIDATES {
            return Err(PyValueError::new_err(format!(
                "combat-entry floor chance scan requires 1..={MAX_COMBAT_ENTRY_FLOOR_CHANCE_CANDIDATES} candidates"
            )));
        }
        let artifact =
            CombatLearningRootBatchArtifactV1::decode(payload, expected_roots, max_bytes)
                .map_err(value_error)?;
        let checkpoints = artifact.into_checkpoints().map_err(value_error)?;
        let source = checkpoints.get(source_slot).cloned().ok_or_else(|| {
            PyValueError::new_err(format!(
                "combat public-chance source slot {source_slot} is out of range"
            ))
        })?;
        let population = combat_entry_floor_chance_population_v1(
            source,
            candidate_floor_seed_base_start,
            max_candidates,
            required_particles,
        )
        .map_err(value_error)?;
        if !population.is_complete(required_particles) {
            return Err(PyValueError::new_err(format!(
                "combat-entry floor chance scan accepted {}/{} unique particles after {} candidates (public matches {}, duplicate private states {})",
                population.checkpoints().len(),
                required_particles,
                population.attempted_candidate_count,
                population.public_match_count,
                population.duplicate_private_state_count,
            )));
        }
        let accepted_floor_seed_bases = population.accepted_floor_seed_bases.clone();
        let attempted_candidate_count = population.attempted_candidate_count;
        let public_match_count = population.public_match_count;
        let duplicate_private_state_count = population.duplicate_private_state_count;
        let env = Self::from_combat_root_checkpoints(
            population.into_checkpoints(),
            CombatLearningPotionPolicyV1::All,
        )
        .map_err(value_error)?;
        Ok((
            env,
            accepted_floor_seed_bases,
            attempted_candidate_count,
            public_match_count,
            duplicate_private_state_count,
        ))
    }

    /// Merge canonical single-root payloads while keeping checkpoints opaque.
    #[staticmethod]
    #[pyo3(signature = (payloads, *, max_bytes))]
    fn merge_combat_root_artifact_bytes<'py>(
        py: Python<'py>,
        payloads: Vec<Vec<u8>>,
        max_bytes: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        if payloads.is_empty() || payloads.len() > MAX_EXPORTED_COMBAT_ROOTS {
            return Err(PyValueError::new_err(format!(
                "combat root merge requires 1..={MAX_EXPORTED_COMBAT_ROOTS} payloads"
            )));
        }
        let artifact = CombatLearningRootBatchArtifactV1::merge_single_root_payloads(
            payloads.iter().map(Vec::as_slice),
            max_bytes,
        )
        .map_err(value_error)?;
        let payload = artifact.encode(max_bytes).map_err(value_error)?;
        Ok(PyBytes::new(py, &payload))
    }

    /// Return canonical potion identities accepted by typed root selectors.
    #[staticmethod]
    fn supported_potion_ids() -> Vec<String> {
        sts_oracle_eval::content::potions::ALL_POTIONS
            .iter()
            .map(|potion| potion.stable_id().to_string())
            .collect()
    }

    /// Normalize one typed encounter identity accepted by root selectors.
    #[staticmethod]
    fn canonical_encounter_id(raw: &str) -> PyResult<String> {
        sts_oracle_eval::sim::combat_start::encounter_id_from_input(raw)
            .map(|encounter| encounter.stable_id().to_string())
            .map_err(value_error)
    }

    /// Export selected current undecoded combat slots as one opaque root artifact.
    ///
    /// Python may persist or forward the bytes but never receives simulator
    /// checkpoint fields.
    #[pyo3(signature = (slot_indices, *, max_bytes))]
    fn combat_root_artifact_bytes<'py>(
        &self,
        py: Python<'py>,
        slot_indices: Vec<usize>,
        max_bytes: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let payload = self
            .encode_combat_root_artifact(&slot_indices, max_bytes)
            .map_err(value_error)?;
        Ok(PyBytes::new(py, &payload))
    }

    #[pyo3(signature = (dense_mask=false, semantic=false, legality_bootstrap=false, privileged_node_sl_oracle=false, reward_exit_slots=None))]
    fn decision_batch<'py>(
        &self,
        py: Python<'py>,
        dense_mask: bool,
        semantic: bool,
        legality_bootstrap: bool,
        privileged_node_sl_oracle: bool,
        reward_exit_slots: Option<Vec<usize>>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let reward_exit_slots = reward_exit_slots.unwrap_or_default();
        let slots = reward_exit_slots.iter().copied().collect::<BTreeSet<_>>();
        if slots.len() != reward_exit_slots.len()
            || slots.iter().any(|&slot| slot >= self.pool.slot_count())
            || (!slots.is_empty() && !legality_bootstrap)
        {
            return Err(PyValueError::new_err(
                "invalid baseline reward-exit memory slots",
            ));
        }
        let snapshot = self.decision_snapshot()?;
        let semantic = semantic.then(|| self.semantic_snapshot()).transpose()?;
        let legality_bootstrap_behavior = legality_bootstrap
            .then(|| self.legality_bootstrap_behavior_snapshot(&snapshot, &slots))
            .transpose()?;
        let node_sl_oracle = privileged_node_sl_oracle
            .then(|| self.privileged_node_sl_oracle_snapshot(&snapshot))
            .transpose()?;
        decision_batch_dict(
            py,
            snapshot,
            dense_mask,
            semantic,
            legality_bootstrap_behavior,
            node_sl_oracle,
        )
    }

    /// Return one sanitized content identity for every current policy row.
    /// Root and symbolic-selection rows are aligned to `decision_batch()`;
    /// exact simulator handles remain private to the bridge resolution table.
    fn public_information_snapshots<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let source = LearningBatchDecisionSource::new(&self.pool);
        let captured = public_information_snapshots_from_learning_source(&source, &self.states)
            .map_err(value_error)?;
        let snapshots = PyList::empty(py);
        for captured in captured {
            let view = Py::new(
                py,
                PyLearningPublicDecisionSnapshotV1 {
                    phase: captured.phase,
                    inner: captured.snapshot,
                },
            )?;
            snapshots.append((captured.slot_index, view))?;
        }
        Ok(snapshots)
    }

    /// Return the exact typed strategic candidates for one current root decision.
    ///
    /// Combat, terminal, ready, and symbolic-selection slots return `None`.
    fn strategic_decision_audit_json(&self, slot_index: usize) -> PyResult<Option<String>> {
        let source = LearningBatchDecisionSource::new(&self.pool);
        strategic_decision_audit_json_from_source(&source, &self.states, slot_index)
            .map_err(value_error)
    }

    /// Return typed public combat candidates for one current combat decision.
    /// Exact simulator handles remain private to the bridge.
    fn combat_decision_audit_json(&self, slot_index: usize) -> PyResult<Option<String>> {
        let source = LearningBatchDecisionSource::new(&self.pool);
        combat_decision_audit_json_from_source(&source, &self.states, slot_index)
            .map_err(value_error)
    }

    fn choose(&mut self, decision_token_ids: Vec<String>, ordinals: Vec<usize>) -> PyResult<()> {
        let source = LearningBatchDecisionSource::new(&self.pool);
        bridge_decision::choose_bridge_tokenized_ordinals(
            &source,
            &mut self.states,
            decision_token_ids,
            ordinals,
        )
        .map_err(value_error)
    }

    #[pyo3(signature = (*, execution_inputs_json=None))]
    fn step<'py>(
        &mut self,
        py: Python<'py>,
        execution_inputs_json: Option<&str>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let paths = learning_ready_paths(&self.states)?;
        let transitions = match execution_inputs_json {
            Some(json) => {
                let inputs: Vec<LearningExecutionInputsV1> =
                    serde_json::from_str(json).map_err(value_error)?;
                self.pool.step_active_paths_with_inputs_v1(paths, inputs)
            }
            None => self.pool.step_active_paths_v3(paths),
        }
        .map_err(runtime_error)?;
        self.states = states_from_source(&self.pool).map_err(runtime_error)?;

        learning_transitions_to_python(py, &transitions)
    }

    /// Private diagnostic identities; never part of model observations.
    fn continuation_fingerprints(&self) -> Vec<String> {
        self.pool.continuation_fingerprints_v1()
    }

    #[staticmethod]
    fn fixed_combat_options_json() -> PyResult<String> {
        serde_json::to_string(&FixedCombatOptionsV1::default()).map_err(runtime_error)
    }

    /// Explicit oracle macro; ordinary step and checkpoint semantics are unchanged.
    /// Unresolved has no transition columns and leaves pending choices intact.
    fn step_with_fixed_combat<'py>(
        &mut self,
        py: Python<'py>,
        options_json: &str,
    ) -> PyResult<Bound<'py, PyDict>> {
        let options: FixedCombatOptionsV1 =
            serde_json::from_str(options_json).map_err(value_error)?;
        let paths = learning_ready_paths(&self.states)?;
        let mut trial = self.pool.clone();
        let outcome = py
            .detach(|| trial.step_with_fixed_combat_v1(paths, options))
            .map_err(runtime_error)?;
        match outcome {
            FixedCombatStepV1::Advanced {
                transitions,
                evidence,
            } => {
                let states = states_from_source(&trial).map_err(runtime_error)?;
                let result = learning_transitions_to_python(py, &transitions)?;
                result.set_item("status", "advanced")?;
                result.set_item(
                    "fixed_combat_evidence_json",
                    serde_json::to_string(&evidence).map_err(runtime_error)?,
                )?;
                self.pool = trial;
                self.states = states;
                Ok(result)
            }
            FixedCombatStepV1::Unresolved {
                slot_index,
                evidence,
            } => {
                let result = PyDict::new(py);
                result.set_item("status", "unresolved")?;
                result.set_item("unresolved_slot_index", slot_index)?;
                result.set_item(
                    "fixed_combat_evidence_json",
                    serde_json::to_string(&evidence).map_err(runtime_error)?,
                )?;
                Ok(result)
            }
        }
    }

    /// Independent exact exits for continuation research. The source batch,
    /// including every pending choice in other slots, is unchanged.
    fn fixed_combat_branches<'py>(
        &self,
        py: Python<'py>,
        slot_index: usize,
        options_json: &str,
    ) -> PyResult<Bound<'py, PyDict>> {
        if !matches!(self.states.get(slot_index), Some(BridgeSlotState::Root)) {
            return Err(PyValueError::new_err(format!(
                "slot {slot_index} must be at an undecoded combat root decision"
            )));
        }
        let options: FixedCombatOptionsV1 =
            serde_json::from_str(options_json).map_err(value_error)?;
        let branches = py
            .detach(|| self.pool.fixed_combat_branches_v1(slot_index, options))
            .map_err(runtime_error)?;
        fixed_combat_branches_to_python(py, branches, slot_index)
    }

    fn start_fixed_combat_research(
        &self,
        slot_index: usize,
        options_json: &str,
    ) -> PyResult<FixedCombatResearch> {
        if !matches!(self.states.get(slot_index), Some(BridgeSlotState::Root)) {
            return Err(PyValueError::new_err(format!(
                "slot {slot_index} must be at an undecoded combat root decision"
            )));
        }
        let options: FixedCombatOptionsV1 =
            serde_json::from_str(options_json).map_err(value_error)?;
        Ok(FixedCombatResearch {
            research: self
                .pool
                .start_fixed_combat_research_v1(slot_index, options)
                .map_err(runtime_error)?,
        })
    }

    /// Adopt only the original combat witness after caller-owned research.
    /// Other slots' pending symbolic or ready choices remain unchanged.
    fn adopt_fixed_combat_branch<'py>(
        &mut self,
        py: Python<'py>,
        choice: PyRef<'_, FixedCombatBranchChoice>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let choice = choice.choice.clone();
        let slot_index = choice.slot_index();
        if !matches!(self.states.get(slot_index), Some(BridgeSlotState::Root)) {
            return Err(PyValueError::new_err(format!(
                "slot {slot_index} must be at an undecoded combat root decision"
            )));
        }
        let mut trial = self.pool.clone();
        let receipt = py
            .detach(|| trial.adopt_fixed_combat_branch_v1(&choice))
            .map_err(runtime_error)?;
        let terminal = trial
            .boundary(slot_index)
            .ok_or_else(|| PyRuntimeError::new_err("adopted combat slot disappeared"))?
            .is_terminal();
        let next_token = if terminal {
            None
        } else {
            Some(
                trial
                    .decision_token(slot_index)
                    .map_err(runtime_error)?
                    .token_id,
            )
        };
        let result = PyDict::new(py);
        result.set_item("information_access", "oracle_exact_combat")?;
        result.set_item("source_slot_index", slot_index)?;
        result.set_item("source_decision_token_id", &choice.source().token_id)?;
        result.set_item("next_decision_token_id", next_token)?;
        result.set_item(
            "fixed_combat_receipt_json",
            serde_json::to_string(&receipt).map_err(runtime_error)?,
        )?;
        self.pool = trial;
        self.states[slot_index] = if terminal {
            BridgeSlotState::Terminal
        } else {
            BridgeSlotState::Root
        };
        Ok(result)
    }

    #[pyo3(signature = (slot_index, replicate_count, potion_slots=None))]
    fn combat_group(
        &self,
        slot_index: usize,
        replicate_count: usize,
        potion_slots: Option<Vec<usize>>,
    ) -> PyResult<CombatLearningBatchEnv> {
        if !matches!(self.states.get(slot_index), Some(BridgeSlotState::Root)) {
            return Err(PyValueError::new_err(format!(
                "slot {slot_index} must be at an undecoded root decision"
            )));
        }
        let checkpoint = self
            .pool
            .session_checkpoint_slot(slot_index)
            .map_err(runtime_error)?;
        let root = CombatLearningRootV1::from_checkpoint(checkpoint).map_err(value_error)?;
        CombatLearningBatchEnv::from_root_with_potion_slots(&root, replicate_count, potion_slots)
            .map_err(value_error)
    }

    /// Return every current undecoded combat root without creating replicate groups.
    fn combat_root_contexts<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let contexts = PyList::empty(py);
        for (slot_index, state) in self.states.iter().enumerate() {
            if !matches!(state, BridgeSlotState::Root)
                || !matches!(
                    self.pool.boundary(slot_index),
                    Some(LearningBoundaryV1::Combat { .. })
                )
            {
                continue;
            }
            let context = self
                .pool
                .combat_root_context(slot_index)
                .map_err(runtime_error)?;
            let view = Py::new(py, PyCombatLearningRootContextV1::from_context(context))?;
            contexts.append((slot_index, view))?;
        }
        Ok(contexts)
    }

    /// Return auditable deck, upgrade, relic, and ascension facts for one root.
    ///
    /// The exact session stays opaque.  Python receives only stable domain
    /// identities needed to review curriculum composition.
    fn combat_root_audit(&self, slot_index: usize) -> PyResult<PyCombatLearningRootAuditV1> {
        if !matches!(self.states.get(slot_index), Some(BridgeSlotState::Root))
            || !matches!(
                self.pool.boundary(slot_index),
                Some(LearningBoundaryV1::Combat { .. })
            )
        {
            return Err(PyValueError::new_err(format!(
                "slot {slot_index} must be at an undecoded combat root"
            )));
        }
        let context = self
            .pool
            .combat_root_context(slot_index)
            .map_err(runtime_error)?;
        let public = self
            .pool
            .public_run_context(slot_index)
            .map_err(runtime_error)?;
        let encounter_id = public.encounter_id.ok_or_else(|| {
            PyRuntimeError::new_err("combat root audit has no encounter identity")
        })?;
        let session = self
            .pool
            .session_checkpoint_slot(slot_index)
            .map_err(runtime_error)?
            .into_session()
            .map_err(runtime_error)?;
        let active = session
            .active_combat
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("combat root audit has no active combat"))?;
        let deck = &active.combat_state.meta.master_deck_snapshot;
        let relics = &active.combat_state.entities.player.relics;
        if deck.len() != context.master_deck_card_count as usize
            || relics.len() != context.relic_count as usize
            || active.combat_state.meta.ascension_level != context.ascension_level
            || public.ascension_level != context.ascension_level
            || public.act != context.act
            || public.floor != context.floor
            || public.hp != context.hp
            || public.max_hp != context.max_hp
        {
            return Err(PyRuntimeError::new_err(
                "combat root audit disagrees with its compact context",
            ));
        }
        Ok(PyCombatLearningRootAuditV1 {
            seed: public.seed,
            act: public.act,
            floor: public.floor,
            ascension_level: context.ascension_level,
            hp: public.hp,
            max_hp: public.max_hp,
            potion_ids: potion_id_names(&public.potion_ids),
            encounter_id: encounter_id.stable_id().to_string(),
            monster_ids: public
                .monster_ids
                .iter()
                .map(|monster| monster.stable_id().to_string())
                .collect(),
            is_elite_fight: context.is_elite_fight,
            is_boss_fight: context.is_boss_fight,
            master_deck_cards: deck
                .iter()
                .map(|card| (card.id.stable_id().to_string(), card.upgrades))
                .collect(),
            relic_ids: relics
                .iter()
                .map(|relic| relic.id.stable_id().to_string())
                .collect(),
        })
    }

    /// Return compact public run facts for every slot without cloning sessions.
    fn public_run_contexts<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let contexts = PyList::empty(py);
        for slot_index in 0..self.pool.slot_count() {
            let context = self
                .pool
                .public_run_context(slot_index)
                .map_err(runtime_error)?;
            let view = Py::new(py, PyLearningPublicRunContextV1 { inner: context })?;
            contexts.append((slot_index, view))?;
        }
        Ok(contexts)
    }

    fn reset_slot(&mut self, slot_index: usize, seed: u64) -> PyResult<()> {
        self.reset_slots(vec![slot_index], vec![seed])
    }

    fn reset_slots(&mut self, slot_indices: Vec<usize>, seeds: Vec<u64>) -> PyResult<()> {
        self.reset_slots_checkpointed(slot_indices, seeds)
            .map(|_| ())
    }

    fn reset_slots_checkpointed(
        &mut self,
        slot_indices: Vec<usize>,
        seeds: Vec<u64>,
    ) -> PyResult<LearningCheckpointBatch> {
        if slot_indices.len() != seeds.len() {
            return Err(PyValueError::new_err(format!(
                "expected {} reset seeds, received {}",
                slot_indices.len(),
                seeds.len()
            )));
        }
        let mut seen = BTreeSet::new();
        for slot_index in &slot_indices {
            if !seen.insert(*slot_index) {
                return Err(PyValueError::new_err(format!(
                    "slot {slot_index} appears more than once in reset batch"
                )));
            }
            if !matches!(
                self.states.get(*slot_index),
                Some(BridgeSlotState::Terminal)
            ) {
                return Err(PyValueError::new_err(format!(
                    "slot {slot_index} is not terminal"
                )));
            }
        }
        let mut checkpoints = Vec::with_capacity(slot_indices.len());
        let mut replacements = Vec::with_capacity(slot_indices.len());
        for (slot_index, seed) in slot_indices.iter().copied().zip(seeds) {
            let config = self
                .pool
                .fresh_run_config(slot_index, seed)
                .map_err(runtime_error)?;
            let env = LearningEnvV3::new(config);
            checkpoints.push(LearningSlotCheckpoint {
                source_slot_index: slot_index,
                candidate_surface: self.pool.candidate_surface(),
                env: env.checkpoint(),
                bridge_state: BridgeSlotState::Root,
            });
            replacements.push((slot_index, env));
        }
        self.pool
            .replace_slots(replacements)
            .map_err(runtime_error)?;
        for slot_index in slot_indices {
            self.states[slot_index] = BridgeSlotState::Root;
        }
        Ok(LearningCheckpointBatch { checkpoints })
    }

    fn checkpoint_slot(&self, slot_index: usize) -> PyResult<LearningSlotCheckpoint> {
        let bridge_state = self.states.get(slot_index).cloned().ok_or_else(|| {
            PyValueError::new_err(format!(
                "slot {slot_index} is outside 0..{}",
                self.states.len()
            ))
        })?;
        let env = self
            .pool
            .checkpoint_slot(slot_index)
            .map_err(runtime_error)?;
        Ok(LearningSlotCheckpoint {
            source_slot_index: slot_index,
            candidate_surface: self.pool.candidate_surface(),
            env,
            bridge_state,
        })
    }

    fn checkpoint_slots(&self, slot_indices: Vec<usize>) -> PyResult<LearningCheckpointBatch> {
        let mut seen = BTreeSet::new();
        let mut checkpoints = Vec::with_capacity(slot_indices.len());
        for slot_index in slot_indices {
            if !seen.insert(slot_index) {
                return Err(PyValueError::new_err(format!(
                    "slot {slot_index} appears more than once in checkpoint batch"
                )));
            }
            checkpoints.push(self.checkpoint_slot(slot_index)?);
        }
        Ok(LearningCheckpointBatch { checkpoints })
    }

    fn restore_slot(
        &mut self,
        slot_index: usize,
        checkpoint: PyRef<'_, LearningSlotCheckpoint>,
    ) -> PyResult<()> {
        if slot_index >= self.states.len() {
            return Err(PyValueError::new_err(format!(
                "slot {slot_index} is outside 0..{}",
                self.states.len()
            )));
        }
        if checkpoint.source_slot_index != slot_index {
            return Err(PyValueError::new_err(format!(
                "checkpoint belongs to slot {}, not slot {slot_index}",
                checkpoint.source_slot_index
            )));
        }
        self.restore_slot_checkpoint(slot_index, &checkpoint)
            .map_err(runtime_error)
    }

    fn restore_slots(
        &mut self,
        slot_indices: Vec<usize>,
        checkpoints: PyRef<'_, LearningCheckpointBatch>,
    ) -> PyResult<()> {
        if slot_indices.len() != checkpoints.checkpoints.len() {
            return Err(PyValueError::new_err(format!(
                "expected {} target slots, received {}",
                checkpoints.checkpoints.len(),
                slot_indices.len()
            )));
        }
        let mut seen = BTreeSet::new();
        let mut replacements = Vec::with_capacity(slot_indices.len());
        for (slot_index, checkpoint) in slot_indices.iter().copied().zip(&checkpoints.checkpoints) {
            if checkpoint.candidate_surface != self.pool.candidate_surface() {
                return Err(value_error(
                    "checkpoint candidate surface differs from destination",
                ));
            }
            if slot_index >= self.states.len() {
                return Err(PyValueError::new_err(format!(
                    "slot {slot_index} is outside 0..{}",
                    self.states.len()
                )));
            }
            if !seen.insert(slot_index) {
                return Err(PyValueError::new_err(format!(
                    "slot {slot_index} appears more than once in restore batch"
                )));
            }
            if checkpoint.source_slot_index != slot_index {
                return Err(PyValueError::new_err(format!(
                    "checkpoint belongs to slot {}, not slot {slot_index}",
                    checkpoint.source_slot_index
                )));
            }
            let env =
                LearningEnvV3::from_checkpoint(checkpoint.env.clone()).map_err(value_error)?;
            replacements.push((slot_index, env));
        }
        self.pool
            .replace_slots(replacements)
            .map_err(runtime_error)?;
        for (slot_index, checkpoint) in slot_indices.into_iter().zip(&checkpoints.checkpoints) {
            self.states[slot_index] = checkpoint.bridge_state.clone();
        }
        Ok(())
    }
}

impl LearningBatchEnv {
    fn from_seeds_with_potion_policy(
        seeds: Vec<u64>,
        ascension_level: u8,
        potion_policy: CombatLearningPotionPolicyV1,
        final_act: bool,
        candidate_surface: LearningCandidateSurfaceV1,
        player_class: &'static str,
        neow_eligibility: sts_oracle_learning::content::events::neow::NeowEligibilityV1,
    ) -> PyResult<Self> {
        if ascension_level > 20 {
            return Err(PyValueError::new_err(
                "ascension_level must be between 0 and 20",
            ));
        }
        let envs = seeds.into_iter().map(|seed| {
            LearningEnvV3::new(RunControlConfig {
                seed,
                ascension_level,
                final_act,
                player_class,
                neow_eligibility,
                ..RunControlConfig::default()
            })
        });
        let pool = LearningEnvPoolV3::from_envs_with_potion_policy(envs, potion_policy)
            .map_err(runtime_error)?
            .with_candidate_surface(candidate_surface);
        let states = states_from_source(&pool).map_err(runtime_error)?;
        Ok(Self { pool, states })
    }

    fn encode_combat_root_artifact(
        &self,
        slot_indices: &[usize],
        max_bytes: usize,
    ) -> Result<Vec<u8>, String> {
        if slot_indices.is_empty() || slot_indices.len() > MAX_EXPORTED_COMBAT_ROOTS {
            return Err(format!(
                "combat root export count must be in 1..={MAX_EXPORTED_COMBAT_ROOTS}"
            ));
        }
        let mut seen = BTreeSet::new();
        let mut checkpoints = Vec::with_capacity(slot_indices.len());
        for &slot_index in slot_indices {
            if !seen.insert(slot_index) {
                return Err(format!(
                    "combat root export slot {slot_index} appears more than once"
                ));
            }
            if !matches!(self.states.get(slot_index), Some(BridgeSlotState::Root))
                || !matches!(
                    self.pool.boundary(slot_index),
                    Some(LearningBoundaryV1::Combat { .. })
                )
            {
                return Err(format!(
                    "combat root export slot {slot_index} must be at an undecoded combat root"
                ));
            }
            checkpoints.push(
                self.pool
                    .session_checkpoint_slot(slot_index)
                    .map_err(|error| error.to_string())?,
            );
        }
        CombatLearningRootBatchArtifactV1::from_checkpoints(checkpoints)?.encode(max_bytes)
    }

    fn decode_combat_root_artifact(
        payload: &[u8],
        expected_roots: usize,
        max_bytes: usize,
        potion_policy: CombatLearningPotionPolicyV1,
    ) -> Result<Self, String> {
        let artifact =
            CombatLearningRootBatchArtifactV1::decode(payload, expected_roots, max_bytes)?;
        Self::from_combat_root_checkpoints(artifact.into_checkpoints()?, potion_policy)
    }

    fn from_combat_root_checkpoints(
        checkpoints: Vec<RunControlSessionCheckpointV1>,
        potion_policy: CombatLearningPotionPolicyV1,
    ) -> Result<Self, String> {
        let envs = checkpoints
            .into_iter()
            .map(|checkpoint| checkpoint.into_session().map(LearningEnvV3::from_session))
            .collect::<Result<Vec<_>, _>>()?;
        let pool = LearningEnvPoolV3::from_envs_with_potion_policy(envs, potion_policy)
            .map_err(|error| error.to_string())?;
        for slot_index in 0..pool.slot_count() {
            if !matches!(
                pool.boundary(slot_index),
                Some(LearningBoundaryV1::Combat { .. })
            ) {
                return Err(format!(
                    "combat learning root artifact slot {slot_index} is not at a combat boundary"
                ));
            }
        }
        let states = states_from_source(&pool)?;
        if let Some(slot_index) = states
            .iter()
            .position(|state| !matches!(state, BridgeSlotState::Root))
        {
            return Err(format!(
                "combat learning root artifact slot {slot_index} is not at an undecoded root"
            ));
        }
        Ok(Self { pool, states })
    }

    fn restore_slot_checkpoint(
        &mut self,
        slot_index: usize,
        checkpoint: &LearningSlotCheckpoint,
    ) -> Result<(), String> {
        if checkpoint.candidate_surface != self.pool.candidate_surface() {
            return Err("checkpoint candidate surface differs from destination".into());
        }
        if slot_index >= self.states.len() {
            return Err(format!(
                "slot {slot_index} is outside 0..{}",
                self.states.len()
            ));
        }
        if checkpoint.source_slot_index != slot_index {
            return Err(format!(
                "checkpoint belongs to slot {}, not slot {slot_index}",
                checkpoint.source_slot_index
            ));
        }
        let env = LearningEnvV3::from_checkpoint(checkpoint.env.clone())?;
        self.pool
            .replace_slot(slot_index, env)
            .map_err(|error| error.to_string())?;
        self.states[slot_index] = checkpoint.bridge_state.clone();
        Ok(())
    }
}

impl LearningBatchEnv {
    fn decision_snapshot(&self) -> PyResult<DecisionSnapshot> {
        let source = LearningBatchDecisionSource::new(&self.pool);
        tokenized_decision_snapshot_from_source(&source, &self.states).map_err(runtime_error)
    }

    fn semantic_snapshot(&self) -> PyResult<SemanticBatch> {
        let source = LearningBatchDecisionSource::new(&self.pool);
        semantic_snapshot_from_source(&source, &self.states).map_err(runtime_error)
    }

    fn legality_bootstrap_behavior_snapshot(
        &self,
        snapshot: &DecisionSnapshot,
        reward_exit_slots: &BTreeSet<usize>,
    ) -> PyResult<LegalityBootstrapBehaviorSnapshot> {
        let mut available = Vec::with_capacity(snapshot.slot_indices.len());
        let mut ordinals = Vec::with_capacity(snapshot.slot_indices.len());
        for &slot_index in &snapshot.slot_indices {
            let ordinal = public_legality_bootstrap_ordinal(
                &self.pool,
                &self.states[slot_index],
                slot_index,
                reward_exit_slots.contains(&slot_index),
            )
            .map_err(runtime_error)?;
            available.push(ordinal.is_some());
            ordinals.push(ordinal.unwrap_or(0));
        }
        Ok(LegalityBootstrapBehaviorSnapshot {
            available,
            ordinals,
        })
    }

    fn privileged_node_sl_oracle_snapshot(
        &self,
        snapshot: &DecisionSnapshot,
    ) -> PyResult<PrivilegedNodeSLOracleSnapshot> {
        let mut available = Vec::with_capacity(snapshot.slot_indices.len());
        let mut ordinal_paths = Vec::with_capacity(snapshot.slot_indices.len());
        for &slot_index in &snapshot.slot_indices {
            let path =
                privileged_node_sl_oracle_path(&self.pool, &self.states[slot_index], slot_index)
                    .map_err(runtime_error)?;
            available.push(path.is_some());
            ordinal_paths.push(path.unwrap_or_default());
        }
        Ok(PrivilegedNodeSLOracleSnapshot {
            available,
            ordinal_paths,
        })
    }
}

fn decision_batch_dict<'py>(
    py: Python<'py>,
    snapshot: DecisionSnapshot,
    dense_mask: bool,
    semantic: Option<SemanticBatch>,
    legality_bootstrap_behavior: Option<LegalityBootstrapBehaviorSnapshot>,
    node_sl_oracle: Option<PrivilegedNodeSLOracleSnapshot>,
) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item(
        "slot_indices",
        usize_array(py, snapshot.slot_indices.clone()),
    )?;
    result.set_item("phase", PyArray1::from_vec(py, snapshot.phases))?;
    result.set_item(
        "candidate_counts",
        usize_array(py, snapshot.candidate_counts.clone()),
    )?;
    result.set_item(
        "candidate_row_splits",
        usize_array(py, snapshot.candidate_row_splits),
    )?;
    if let Some(token_ids) = snapshot.decision_token_ids {
        result.set_item("decision_token_ids", token_ids)?;
        result.set_item(
            "public_snapshot_ids",
            snapshot
                .public_snapshot_ids
                .expect("tokenized decision rows must carry public snapshot ids"),
        )?;
        result.set_item(
            "decision_steps",
            PyArray1::from_vec(
                py,
                snapshot
                    .decision_steps
                    .expect("tokenized decision rows must carry decision steps"),
            ),
        )?;
    }
    if dense_mask {
        let width = snapshot.candidate_counts.iter().copied().max().unwrap_or(0);
        let mut values = vec![false; snapshot.candidate_counts.len().saturating_mul(width)];
        for (row, count) in snapshot.candidate_counts.iter().copied().enumerate() {
            values[row * width..row * width + count].fill(true);
        }
        let mask = Array2::from_shape_vec((snapshot.candidate_counts.len(), width), values)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        result.set_item("dense_action_mask", mask.into_pyarray(py))?;
    }
    if let Some(semantic) = semantic {
        result.set_item("semantic", semantic_dict(py, semantic)?)?;
    }
    if let Some(legality_bootstrap_behavior) = legality_bootstrap_behavior {
        result.set_item(
            "legality_bootstrap_available",
            PyArray1::from_vec(py, legality_bootstrap_behavior.available),
        )?;
        result.set_item(
            "legality_bootstrap_ordinals",
            usize_array(py, legality_bootstrap_behavior.ordinals),
        )?;
    }
    if let Some(node_sl_oracle) = node_sl_oracle {
        result.set_item(
            "privileged_node_sl_oracle_available",
            PyArray1::from_vec(py, node_sl_oracle.available),
        )?;
        result.set_item(
            "privileged_node_sl_oracle_ordinal_paths",
            node_sl_oracle.ordinal_paths,
        )?;
    }
    Ok(result)
}

fn privileged_node_sl_oracle_path(
    pool: &LearningEnvPoolV3,
    state: &BridgeSlotState,
    slot_index: usize,
) -> Result<Option<Vec<usize>>, String> {
    if !matches!(state, BridgeSlotState::Root) {
        return Ok(None);
    }
    let Some(boundary @ LearningBoundaryV1::Strategic { .. }) = pool.boundary(slot_index) else {
        return Ok(None);
    };
    let session = pool
        .session_checkpoint_slot(slot_index)
        .map_err(|error| error.to_string())?
        .into_session()?;
    let decision = LearningModelDecisionV1::from_boundary_with_surface(
        boundary,
        pool.potion_policy(),
        pool.candidate_surface(),
    )
    .map_err(|error| error.to_string())?;
    privileged_node_sl_oracle_path_from_decision_v1(&session, &decision)
}

fn public_legality_bootstrap_ordinal(
    pool: &LearningEnvPoolV3,
    state: &BridgeSlotState,
    slot_index: usize,
    after_reward_proceed: bool,
) -> Result<Option<usize>, String> {
    let boundary = pool
        .boundary(slot_index)
        .ok_or_else(|| format!("missing pool slot {slot_index}"))?;
    let decision = LearningModelDecisionV1::from_boundary_with_surface(
        boundary,
        pool.potion_policy(),
        pool.candidate_surface(),
    )
    .map_err(|error| error.to_string())?;
    Ok(match state {
        BridgeSlotState::Root => Some(if after_reward_proceed {
            public_deterministic_baseline_after_reward_proceed_v1(&decision).candidate_ordinal
        } else {
            public_deterministic_baseline_decision_v1(&decision).candidate_ordinal
        }),
        BridgeSlotState::Selection { draft, .. } => Some(
            public_deterministic_baseline_selection_decision_v1(decision.observation, draft)
                .candidate_ordinal,
        ),
        BridgeSlotState::Terminal | BridgeSlotState::Ready { .. } => None,
    })
}

fn usize_array(py: Python<'_>, values: Vec<usize>) -> Bound<'_, PyArray1<u64>> {
    PyArray1::from_vec(
        py,
        values
            .into_iter()
            .map(|value| u64::try_from(value).unwrap_or(u64::MAX))
            .collect(),
    )
}

fn semantic_dict(py: Python<'_>, batch: SemanticBatch) -> PyResult<Bound<'_, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("schema_version", SEMANTIC_SCHEMA_VERSION)?;
    result.set_item("completeness", PyArray1::from_vec(py, batch.completeness))?;

    let token = PyDict::new(py);
    token.set_item("row_splits", PyArray1::from_vec(py, batch.token_row_splits))?;
    token.set_item("kind", PyArray1::from_vec(py, batch.token_kinds))?;
    token.set_item("scope", PyArray1::from_vec(py, batch.token_scopes))?;
    result.set_item("token", token)?;

    let categorical = PyDict::new(py);
    categorical.set_item(
        "token_indices",
        PyArray1::from_vec(py, batch.categorical.token_indices),
    )?;
    categorical.set_item("field", PyArray1::from_vec(py, batch.categorical.fields))?;
    categorical.set_item("value", PyArray1::from_vec(py, batch.categorical.values))?;
    result.set_item("categorical", categorical)?;

    let scalar = PyDict::new(py);
    scalar.set_item(
        "token_indices",
        PyArray1::from_vec(py, batch.scalar.token_indices),
    )?;
    scalar.set_item("field", PyArray1::from_vec(py, batch.scalar.fields))?;
    scalar.set_item("value", PyArray1::from_vec(py, batch.scalar.values))?;
    result.set_item("scalar", scalar)?;

    let relation = PyDict::new(py);
    relation.set_item(
        "source_token_indices",
        PyArray1::from_vec(py, batch.relation.source_token_indices),
    )?;
    relation.set_item("relation", PyArray1::from_vec(py, batch.relation.relations))?;
    relation.set_item(
        "target_token_indices",
        PyArray1::from_vec(py, batch.relation.target_token_indices),
    )?;
    result.set_item("relation", relation)?;
    result.set_item(
        "candidate_token_indices",
        PyArray1::from_vec(py, batch.candidate_token_indices),
    )?;
    Ok(result)
}

#[pyclass(name = "CompleteTurnExactEnv")]
struct PyCompleteTurnExactEnvV1 {
    inner: CompleteTurnExactEnvV1,
}

#[pyclass(name = "CompleteTurnMaterializedOutcome")]
struct PyCompleteTurnMaterializedOutcomeV1 {
    inner: CombatWitnessMaterializedOutcomeV1,
}

#[pymethods]
impl PyCompleteTurnMaterializedOutcomeV1 {
    fn strictly_dominates(&self, other: &Self) -> bool {
        self.inner.strictly_dominates(&other.inner)
    }

    fn summary<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let result = PyDict::new(py);
        result.set_item("final_hp", self.inner.final_hp)?;
        result.set_item("final_gold", self.inner.final_gold)?;
        result.set_item(
            "starting_potion_expenditure_slots",
            self.inner.starting_potion_expenditure_slots,
        )?;
        result.set_item(
            "total_potion_expenditures",
            self.inner.total_potion_expenditures,
        )?;
        result.set_item(
            "generated_potions",
            self.inner
                .generated_potions
                .iter()
                .map(|(slot, potion)| (*slot, potion.id.stable_id().to_string()))
                .collect::<Vec<_>>(),
        )?;
        result.set_item(
            "final_relics",
            self.inner
                .final_relics
                .iter()
                .map(|relic| relic.id.stable_id().to_string())
                .collect::<Vec<_>>(),
        )?;
        result.set_item(
            "meta_changes",
            self.inner
                .meta_changes
                .iter()
                .map(|change| serde_json::to_string(change).map_err(runtime_error))
                .collect::<PyResult<Vec<_>>>()?,
        )?;
        result.set_item(
            "pending_rewards",
            self.inner
                .pending_rewards
                .iter()
                .map(|reward| serde_json::to_string(reward).map_err(runtime_error))
                .collect::<PyResult<Vec<_>>>()?,
        )?;
        result.set_item("combat_mugged", self.inner.combat_mugged)?;
        let persistent = PyDict::new(py);
        persistent.set_item("max_hp", self.inner.persistent.max_hp)?;
        persistent.set_item(
            "recoverable_gold_delta",
            self.inner.persistent.recoverable_gold_delta,
        )?;
        persistent.set_item(
            "unrecovered_stolen_gold",
            self.inner.persistent.unrecovered_stolen_gold,
        )?;
        persistent.set_item(
            "ritual_dagger_value",
            self.inner.persistent.ritual_dagger_value,
        )?;
        persistent.set_item(
            "genetic_algorithm_value",
            self.inner.persistent.genetic_algorithm_value,
        )?;
        persistent.set_item(
            "external_burden_count",
            self.inner.persistent.external_burden_count,
        )?;
        result.set_item("persistent", persistent)?;
        Ok(result)
    }
}

#[pymethods]
impl PyCompleteTurnExactEnvV1 {
    #[staticmethod]
    #[pyo3(signature = (
        payload,
        *,
        allow_potions,
        max_inner_nodes = 256,
        max_partial_frontier_states = 32,
        max_end_states = 24,
        per_bucket_limit = 24,
    ))]
    fn from_case_bytes(
        payload: &[u8],
        allow_potions: bool,
        max_inner_nodes: usize,
        max_partial_frontier_states: usize,
        max_end_states: usize,
        per_bucket_limit: usize,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: CompleteTurnExactEnvV1::from_case_json_with_limits(
                payload,
                allow_potions,
                max_inner_nodes,
                max_partial_frontier_states,
                max_end_states,
                per_bucket_limit,
            )
            .map_err(value_error)?,
        })
    }

    fn decision_batch<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let decision = self.inner.observe().map_err(runtime_error)?.clone();
        let candidate_count = u64::try_from(decision.candidates.len()).map_err(value_error)?;
        let mut builder = SemanticBatchBuilder::new();
        builder
            .push_complete_turn_decision(&decision)
            .map_err(value_error)?;

        let result = PyDict::new(py);
        result.set_item("slot_indices", PyArray1::from_vec(py, vec![0_u64]))?;
        result.set_item("phase", PyArray1::from_vec(py, vec![PHASE_COMBAT_ROOT]))?;
        result.set_item(
            "candidate_counts",
            PyArray1::from_vec(py, vec![candidate_count]),
        )?;
        result.set_item(
            "candidate_row_splits",
            PyArray1::from_vec(py, vec![0_u64, candidate_count]),
        )?;
        result.set_item("semantic", semantic_dict(py, builder.finish())?)?;
        Ok(result)
    }

    /// Expands and encodes one complete action-depth layer. Every candidate
    /// is scored in one ragged semantic batch; Python returns only the
    /// observation-local ordinals retained for the next depth.
    fn prefix_layer_batch<'py>(&mut self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyDict>>> {
        let Some(layer) = self
            .inner
            .observe_prefix_layer()
            .map_err(runtime_error)?
            .cloned()
        else {
            return Ok(None);
        };
        let candidate_count = u64::try_from(layer.candidates.len()).map_err(value_error)?;
        let mut builder = SemanticBatchBuilder::new();
        builder
            .push_complete_turn_prefix_layer(&layer)
            .map_err(value_error)?;

        let result = PyDict::new(py);
        result.set_item("atomic_depth", layer.atomic_depth)?;
        result.set_item("max_retained_states", layer.max_retained_states)?;
        result.set_item("nodes_expanded", layer.nodes_expanded)?;
        result.set_item("nodes_generated", layer.nodes_generated)?;
        result.set_item("slot_indices", PyArray1::from_vec(py, vec![0_u64]))?;
        result.set_item("phase", PyArray1::from_vec(py, vec![PHASE_COMBAT_ROOT]))?;
        result.set_item(
            "candidate_counts",
            PyArray1::from_vec(py, vec![candidate_count]),
        )?;
        result.set_item(
            "candidate_row_splits",
            PyArray1::from_vec(py, vec![0_u64, candidate_count]),
        )?;
        result.set_item("semantic", semantic_dict(py, builder.finish())?)?;
        Ok(Some(result))
    }

    /// Returns the same public-only typed prefix view that feeds the semantic
    /// encoder. This is a diagnostic surface for human-readable model audits;
    /// it contains no exact handles, RNG state, or private action resolution.
    fn prefix_layer_public_view_json(&mut self) -> PyResult<Option<Vec<u8>>> {
        self.inner
            .observe_prefix_layer()
            .map_err(runtime_error)?
            .map(serde_json::to_vec)
            .transpose()
            .map_err(value_error)
    }

    fn retain_prefix_layer(&mut self, ordinals: Vec<usize>) -> PyResult<()> {
        self.inner
            .retain_prefix_layer(&ordinals)
            .map_err(value_error)
    }

    fn retain_prefix_layer_with_default_fill(&mut self, ordinals: Vec<usize>) -> PyResult<()> {
        self.inner
            .retain_prefix_layer_with_default_fill(&ordinals)
            .map_err(value_error)
    }

    fn matching_prefix_public_actions_ordinal(
        &mut self,
        actions_json: &[u8],
    ) -> PyResult<Option<usize>> {
        let actions: Vec<CompleteTurnSelectedActionV1> =
            serde_json::from_slice(actions_json).map_err(value_error)?;
        self.inner
            .matching_prefix_public_actions_ordinal(&actions)
            .map_err(runtime_error)
    }

    fn matching_prefix_successor_ordinal(&mut self, inputs_json: &[u8]) -> PyResult<Option<usize>> {
        let inputs: Vec<sts_oracle_eval::state::core::ClientInput> =
            serde_json::from_slice(inputs_json).map_err(value_error)?;
        self.inner
            .matching_prefix_successor_ordinal(&inputs)
            .map_err(runtime_error)
    }

    /// Encodes all generated complete-turn endpoints before the bounded
    /// executable surface selector runs. Returned ordinals reserve proposal
    /// capacity and are not execution choices.
    fn completed_endpoint_batch<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let layer = self
            .inner
            .observe_completed_endpoints()
            .map_err(runtime_error)?
            .clone();
        let candidate_count = u64::try_from(layer.candidates.len()).map_err(value_error)?;
        let mut builder = SemanticBatchBuilder::new();
        builder
            .push_complete_turn_completed_endpoint_layer(&layer)
            .map_err(value_error)?;

        let result = PyDict::new(py);
        result.set_item("max_retained_states", layer.max_retained_states)?;
        result.set_item("slot_indices", PyArray1::from_vec(py, vec![0_u64]))?;
        result.set_item("phase", PyArray1::from_vec(py, vec![PHASE_COMBAT_ROOT]))?;
        result.set_item(
            "candidate_counts",
            PyArray1::from_vec(py, vec![candidate_count]),
        )?;
        result.set_item(
            "candidate_row_splits",
            PyArray1::from_vec(py, vec![0_u64, candidate_count]),
        )?;
        result.set_item("semantic", semantic_dict(py, builder.finish())?)?;
        Ok(result)
    }

    /// Returns the public completed-endpoint reservation view before bounded
    /// endpoint selection. The JSON is diagnostic evidence, not an execution
    /// or training-label channel.
    fn completed_endpoint_public_view_json(&mut self) -> PyResult<Vec<u8>> {
        let layer = self
            .inner
            .observe_completed_endpoints()
            .map_err(runtime_error)?;
        serde_json::to_vec(layer).map_err(value_error)
    }

    fn matching_completed_endpoint_public_actions_ordinal(
        &mut self,
        actions_json: &[u8],
    ) -> PyResult<Option<usize>> {
        let actions: Vec<CompleteTurnSelectedActionV1> =
            serde_json::from_slice(actions_json).map_err(value_error)?;
        self.inner
            .matching_completed_endpoint_public_actions_ordinal(&actions)
            .map_err(runtime_error)
    }

    fn matching_completed_endpoint_successor_ordinal(
        &mut self,
        inputs_json: &[u8],
    ) -> PyResult<Option<usize>> {
        let inputs: Vec<sts_oracle_eval::state::core::ClientInput> =
            serde_json::from_slice(inputs_json).map_err(value_error)?;
        self.inner
            .matching_completed_endpoint_successor_ordinal(&inputs)
            .map_err(runtime_error)
    }

    fn retain_completed_endpoints_with_default_fill(
        &mut self,
        ordinals: Vec<usize>,
    ) -> PyResult<()> {
        self.inner
            .retain_completed_endpoints_with_default_fill(&ordinals)
            .map(|_| ())
            .map_err(value_error)
    }

    fn finish_prefix_surface(&mut self) -> PyResult<()> {
        self.inner
            .finish_prefix_surface()
            .map(|_| ())
            .map_err(runtime_error)
    }

    fn matching_complete_turn_public_actions_ordinal(
        &mut self,
        actions_json: &[u8],
    ) -> PyResult<Option<usize>> {
        let actions: Vec<CompleteTurnSelectedActionV1> =
            serde_json::from_slice(actions_json).map_err(value_error)?;
        self.inner
            .matching_complete_turn_public_actions_ordinal(&actions)
            .map_err(runtime_error)
    }

    fn surface_diagnostics<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let diagnostics = self.inner.surface_diagnostics().map_err(runtime_error)?;
        let result = PyDict::new(py);
        result.set_item("max_inner_nodes", diagnostics.max_inner_nodes)?;
        result.set_item(
            "max_partial_frontier_states",
            diagnostics.max_partial_frontier_states,
        )?;
        result.set_item("max_end_states", diagnostics.max_end_states)?;
        result.set_item("per_bucket_limit", diagnostics.per_bucket_limit)?;
        result.set_item("candidate_count", diagnostics.candidate_count)?;
        result.set_item("nodes_expanded", diagnostics.nodes_expanded)?;
        result.set_item("nodes_generated", diagnostics.nodes_generated)?;
        result.set_item("engine_steps", diagnostics.engine_steps)?;
        result.set_item("exact_state_skips", diagnostics.exact_state_skips)?;
        result.set_item("truncated_children", diagnostics.truncated_children)?;
        result.set_item(
            "preselection_candidate_count",
            diagnostics.preselection_candidate_count,
        )?;
        result.set_item(
            "resource_profile_retained_candidate_count",
            diagnostics.resource_profile_retained_candidate_count,
        )?;
        result.set_item(
            "support_retained_candidate_count",
            diagnostics.support_retained_candidate_count,
        )?;
        result.set_item(
            "completed_endpoint_reserved_candidate_count",
            diagnostics.completed_endpoint_reserved_candidate_count,
        )?;
        result.set_item(
            "partial_prefix_scorer_active",
            diagnostics.partial_prefix_scorer_active,
        )?;
        result.set_item(
            "interactive_prefix_selection_active",
            diagnostics.interactive_prefix_selection_active,
        )?;
        result.set_item(
            "completed_endpoint_reservation_active",
            diagnostics.completed_endpoint_reservation_active,
        )?;
        Ok(result)
    }

    fn matching_candidate_ordinal(&mut self, inputs_json: &[u8]) -> PyResult<Option<usize>> {
        let inputs: Vec<sts_oracle_eval::state::core::ClientInput> =
            serde_json::from_slice(inputs_json).map_err(value_error)?;
        self.inner
            .matching_candidate_ordinal(&inputs)
            .map_err(runtime_error)
    }

    fn matching_successor_ordinal(&mut self, inputs_json: &[u8]) -> PyResult<Option<usize>> {
        let inputs: Vec<sts_oracle_eval::state::core::ClientInput> =
            serde_json::from_slice(inputs_json).map_err(value_error)?;
        self.inner
            .matching_successor_ordinal(&inputs)
            .map_err(runtime_error)
    }

    /// Fork the current exact research boundary, including an already cached
    /// candidate surface. Each fork can then commit a different ordinal
    /// without regenerating or mutating its siblings.
    fn fork(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    fn materialized_outcome(&self) -> PyResult<PyCompleteTurnMaterializedOutcomeV1> {
        Ok(PyCompleteTurnMaterializedOutcomeV1 {
            inner: self.inner.materialized_outcome().map_err(value_error)?,
        })
    }

    fn choose<'py>(&mut self, py: Python<'py>, ordinal: usize) -> PyResult<Bound<'py, PyDict>> {
        let step = self.inner.choose(ordinal).map_err(value_error)?;
        complete_turn_step_dict(py, step)
    }

    #[getter]
    fn is_terminal(&self) -> bool {
        self.inner.terminal() != CombatTerminal::Unresolved
    }

    #[getter]
    fn terminal(&self) -> &'static str {
        combat_terminal_name(self.inner.terminal())
    }

    #[getter]
    fn player_hp(&self) -> i32 {
        self.inner.player_hp()
    }

    #[getter]
    fn player_max_hp(&self) -> i32 {
        self.inner.player_max_hp()
    }

    #[getter]
    fn turn_count(&self) -> u32 {
        self.inner.turn_count()
    }
}

fn complete_turn_step_dict(
    py: Python<'_>,
    step: CompleteTurnExactStepV1,
) -> PyResult<Bound<'_, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("chosen_ordinal", step.chosen_ordinal)?;
    result.set_item("boundary", complete_turn_boundary_name(step.boundary))?;
    result.set_item("terminal", combat_terminal_name(step.terminal))?;
    result.set_item("is_terminal", step.terminal != CombatTerminal::Unresolved)?;
    result.set_item("player_hp", step.player_hp)?;
    result.set_item("player_max_hp", step.player_max_hp)?;
    result.set_item("turn_count", step.turn_count)?;
    Ok(result)
}

fn complete_turn_boundary_name(boundary: ExactCompleteTurnProposalBoundaryV1) -> &'static str {
    match boundary {
        ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn => "next_player_turn",
        ExactCompleteTurnProposalBoundaryV1::TerminalWin => "terminal_win",
        ExactCompleteTurnProposalBoundaryV1::TerminalLoss => "terminal_loss",
        ExactCompleteTurnProposalBoundaryV1::Other => "other",
    }
}

fn combat_terminal_name(terminal: CombatTerminal) -> &'static str {
    match terminal {
        CombatTerminal::Win => "win",
        CombatTerminal::Loss => "loss",
        CombatTerminal::Unresolved => "unresolved",
    }
}

#[pyfunction]
fn complete_turn_demonstration_semantic_batch<'py>(
    py: Python<'py>,
    payload: &[u8],
) -> PyResult<Bound<'py, PyDict>> {
    let demonstration: CompleteTurnWinningDemonstrationV1 =
        serde_json::from_slice(payload).map_err(value_error)?;
    demonstration.validate().map_err(value_error)?;

    let mut builder = SemanticBatchBuilder::new();
    let mut candidate_row_splits = Vec::with_capacity(demonstration.decisions.len() + 1);
    let mut demonstrated_ordinals = Vec::with_capacity(demonstration.decisions.len());
    let mut candidate_evidence = Vec::new();
    candidate_row_splits.push(0_u64);
    let mut candidate_count = 0_u64;
    for row in &demonstration.decisions {
        builder
            .push_complete_turn_decision(&row.decision)
            .map_err(value_error)?;
        let row_candidates = u64::try_from(row.decision.candidates.len()).map_err(value_error)?;
        candidate_count = candidate_count
            .checked_add(row_candidates)
            .ok_or_else(|| value_error("complete-turn candidate count overflow"))?;
        candidate_row_splits.push(candidate_count);
        demonstrated_ordinals
            .push(u64::try_from(row.chosen_candidate_ordinal).map_err(value_error)?);
        candidate_evidence.extend(
            row.candidate_evidence
                .iter()
                .map(|evidence| match evidence {
                    CompleteTurnCandidateEvidenceV1::VerifiedPositiveDemonstration => {
                        CompleteTurnCandidateEvidenceKind::VerifiedPositiveDemonstration as u8
                    }
                    CompleteTurnCandidateEvidenceV1::Unknown => {
                        CompleteTurnCandidateEvidenceKind::Unknown as u8
                    }
                }),
        );
    }

    let result = semantic_dict(py, builder.finish())?;
    result.set_item(
        "candidate_row_splits",
        PyArray1::from_vec(py, candidate_row_splits),
    )?;
    result.set_item(
        "demonstrated_ordinals",
        PyArray1::from_vec(py, demonstrated_ordinals),
    )?;
    result.set_item(
        "candidate_evidence",
        PyArray1::from_vec(py, candidate_evidence),
    )?;
    Ok(result)
}

/// Exact-replay a captured whole-combat line and return simulator-owned
/// player-turn groups. Python receives serialized inputs for each verified
/// turn, but never infers turn boundaries from action or state JSON.
#[pyfunction]
fn replay_verified_winning_complete_turn_line<'py>(
    py: Python<'py>,
    case_payload: &[u8],
    actions_payload: &[u8],
) -> PyResult<Bound<'py, PyDict>> {
    let line = replay_verified_winning_complete_turn_line_v1(case_payload, actions_payload)
        .map_err(value_error)?;
    let turns = PyList::empty(py);
    for turn in &line.turns {
        let item = PyDict::new(py);
        item.set_item("from_turn", turn.from_turn)?;
        item.set_item("to_turn", turn.to_turn)?;
        item.set_item("boundary", complete_turn_boundary_name(turn.boundary))?;
        let inputs_json = serde_json::to_vec(&turn.inputs).map_err(value_error)?;
        item.set_item("inputs_json", PyBytes::new(py, &inputs_json))?;
        let input_prefixes_json = PyList::empty(py);
        for prefix_len in 1..=turn.inputs.len() {
            let prefix = serde_json::to_vec(&turn.inputs[..prefix_len]).map_err(value_error)?;
            input_prefixes_json.append(PyBytes::new(py, &prefix))?;
        }
        item.set_item("input_prefixes_json", input_prefixes_json)?;
        turns.append(item)?;
    }
    let result = PyDict::new(py);
    result.set_item("action_count", line.action_count)?;
    result.set_item("turn_count", line.turns.len())?;
    result.set_item("turns", turns)?;
    Ok(result)
}

#[pyfunction]
fn semantic_schema(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("version", SEMANTIC_SCHEMA_VERSION)?;
    result.set_item(
        "completeness",
        numeric_schema_dict(py, SemanticCompleteness::SCHEMA)?,
    )?;
    result.set_item("token_kind", numeric_schema_dict(py, TokenKind::SCHEMA)?)?;
    result.set_item(
        "categorical_field",
        numeric_schema_dict(py, CategoricalField::SCHEMA)?,
    )?;
    result.set_item(
        "scalar_field",
        numeric_schema_dict(py, ScalarField::SCHEMA)?,
    )?;
    result.set_item(
        "relation_kind",
        numeric_schema_dict(py, RelationKind::SCHEMA)?,
    )?;
    result.set_item(
        "context_kind",
        numeric_schema_dict(py, ContextKind::SCHEMA)?,
    )?;
    result.set_item("action_kind", numeric_schema_dict(py, ActionKind::SCHEMA)?)?;
    result.set_item("reward_kind", numeric_schema_dict(py, RewardKind::SCHEMA)?)?;
    result.set_item(
        "combat_action_kind",
        numeric_schema_dict(py, CombatActionKind::SCHEMA)?,
    )?;
    result.set_item("intent_kind", numeric_schema_dict(py, IntentKind::SCHEMA)?)?;
    result.set_item(
        "map_room_kind",
        numeric_schema_dict(py, MapRoomKind::SCHEMA)?,
    )?;
    result.set_item(
        "question_mark_outcome_kind",
        numeric_schema_dict(py, QuestionMarkOutcomeKind::SCHEMA)?,
    )?;
    result.set_item(
        "shop_purchase_status_kind",
        numeric_schema_dict(py, ShopPurchaseStatusKind::SCHEMA)?,
    )?;
    result.set_item(
        "public_counter_kind",
        numeric_schema_dict(py, PublicCounterKind::SCHEMA)?,
    )?;
    result.set_item(
        "card_zone_kind",
        numeric_schema_dict(py, CardZoneKind::SCHEMA)?,
    )?;
    result.set_item(
        "indexed_choice_reason_kind",
        numeric_schema_dict(py, IndexedChoiceReasonKind::SCHEMA)?,
    )?;
    result.set_item(
        "indexed_choice_candidate_kind",
        numeric_schema_dict(py, IndexedChoiceCandidateKind::SCHEMA)?,
    )?;
    result.set_item(
        "selection_reason_kind",
        numeric_schema_dict(py, SelectionReasonKind::SCHEMA)?,
    )?;
    result.set_item(
        "selection_candidate_kind",
        numeric_schema_dict(py, SelectionCandidateKind::SCHEMA)?,
    )?;
    result.set_item(
        "selection_domain_kind",
        numeric_schema_dict(py, SelectionDomainKind::SCHEMA)?,
    )?;
    result.set_item(
        "counter_item_kind",
        numeric_schema_dict(py, CounterItemKind::SCHEMA)?,
    )?;
    result.set_item(
        "complete_turn_boundary_kind",
        numeric_schema_dict(py, CompleteTurnBoundaryKind::SCHEMA)?,
    )?;
    result.set_item(
        "complete_turn_milestone_kind",
        numeric_schema_dict(py, CompleteTurnMilestoneKind::SCHEMA)?,
    )?;
    result.set_item(
        "complete_turn_candidate_evidence",
        numeric_schema_dict(py, CompleteTurnCandidateEvidenceKind::SCHEMA)?,
    )?;
    result.set_item("card_type", numeric_schema_dict(py, CARD_TYPE_SCHEMA)?)?;
    result.set_item(
        "card_combat_upgrade_scope",
        numeric_schema_dict(py, semantic::CARD_COMBAT_UPGRADE_SCOPE_SCHEMA)?,
    )?;
    for (name, schema) in [
        (
            "card_energy_gain_delivery",
            semantic::CARD_ENERGY_GAIN_DELIVERY_SCHEMA,
        ),
        (
            "card_status_creation_destination",
            semantic::CARD_STATUS_CREATION_DESTINATION_SCHEMA,
        ),
        (
            "card_status_creation_timing",
            semantic::CARD_STATUS_CREATION_TIMING_SCHEMA,
        ),
        (
            "card_strength_gain_delivery",
            semantic::CARD_STRENGTH_GAIN_DELIVERY_SCHEMA,
        ),
        (
            "card_strength_conversion",
            semantic::CARD_STRENGTH_CONVERSION_SCHEMA,
        ),
        (
            "card_damage_delivery",
            semantic::CARD_DAMAGE_DELIVERY_SCHEMA,
        ),
        ("card_attack_amount", semantic::CARD_ATTACK_AMOUNT_SCHEMA),
        ("card_attack_hits", semantic::CARD_ATTACK_HITS_SCHEMA),
        ("card_attack_targets", semantic::CARD_ATTACK_TARGETS_SCHEMA),
        (
            "card_attack_play_condition",
            semantic::CARD_ATTACK_PLAY_CONDITION_SCHEMA,
        ),
    ] {
        result.set_item(name, numeric_schema_dict(py, schema)?)?;
    }
    for (name, schema) in [
        ("power_trigger", POWER_TRIGGER_SCHEMA),
        ("power_effect", POWER_EFFECT_SCHEMA),
        ("power_effect_target", POWER_EFFECT_TARGET_SCHEMA),
        ("power_amount_kind", POWER_AMOUNT_KIND_SCHEMA),
        ("power_lifetime", POWER_LIFETIME_SCHEMA),
    ] {
        result.set_item(name, numeric_schema_dict(py, schema)?)?;
    }
    result.set_item("card_rarity", numeric_schema_dict(py, CARD_RARITY_SCHEMA)?)?;
    result.set_item("card_target", numeric_schema_dict(py, CARD_TARGET_SCHEMA)?)?;
    result.set_item("card_tag", numeric_schema_dict(py, CARD_TAG_SCHEMA)?)?;
    result.set_item(
        "card_mechanic_coverage",
        numeric_schema_dict(py, CARD_MECHANIC_COVERAGE_SCHEMA)?,
    )?;
    result.set_item(
        "card_mechanic_role",
        numeric_schema_dict(py, CARD_MECHANIC_ROLE_SCHEMA)?,
    )?;
    result.set_item(
        "potion_rarity",
        numeric_schema_dict(py, POTION_RARITY_SCHEMA)?,
    )?;
    result.set_item(
        "potion_class",
        numeric_schema_dict(py, POTION_CLASS_SCHEMA)?,
    )?;
    result.set_item(
        "potion_mechanic_role",
        numeric_schema_dict(py, POTION_MECHANIC_ROLE_SCHEMA)?,
    )?;

    let vocabulary_sizes = PyDict::new(py);
    for (field, size) in CATEGORICAL_VOCABULARY_SIZES {
        vocabulary_sizes.set_item(*field, *size)?;
    }
    result.set_item("categorical_vocabulary_size", vocabulary_sizes)?;
    let identity_residual_fields = PyDict::new(py);
    for field in IDENTITY_RESIDUAL_CATEGORICAL_FIELDS {
        identity_residual_fields.set_item(*field, 1_i64)?;
    }
    result.set_item(
        "identity_residual_categorical_fields",
        identity_residual_fields,
    )?;

    let domains = PyDict::new(py);
    domains.set_item("card_id", CARD_ID_VOCABULARY_SIZE)?;
    domains.set_item("relic_id", RELIC_ID_VOCABULARY_SIZE)?;
    domains.set_item("potion_id", POTION_ID_VOCABULARY_SIZE)?;
    domains.set_item("encounter_id", ENCOUNTER_ID_VOCABULARY_SIZE)?;
    domains.set_item("event_id", EVENT_ID_VOCABULARY_SIZE)?;
    domains.set_item("enemy_id", ENEMY_ID_VOCABULARY_SIZE)?;
    domains.set_item("power_id", POWER_ID_VOCABULARY_SIZE)?;
    result.set_item("domain_vocabulary_size", domains)?;
    Ok(result)
}

/// Describe the canonical deployable bootstrap without inspecting Rust source.
#[pyfunction]
fn public_legality_bootstrap_manifest(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let manifest = public_deterministic_baseline_manifest_v1();
    let result = PyDict::new(py);
    result.set_item("schema", manifest.schema)?;
    result.set_item("name", manifest.name)?;
    result.set_item("revision", manifest.revision)?;
    result.set_item("lifecycle", manifest.lifecycle)?;
    result.set_item("role", manifest.role)?;
    result.set_item(
        "information_access",
        public_legality_bootstrap_information_access_name(manifest.information_access),
    )?;
    result.set_item("deterministic", manifest.deterministic)?;
    result.set_item(
        "same_public_history_same_action",
        manifest.same_public_history_same_action,
    )?;

    let maintenance = PyDict::new(py);
    maintenance.set_item("clear_defect", manifest.clear_defect_policy)?;
    maintenance.set_item("ambiguous_weakness", manifest.ambiguous_weakness_policy)?;
    maintenance.set_item("prohibited_change", manifest.prohibited_change)?;
    result.set_item("maintenance", maintenance)?;

    let capabilities = PyList::empty(py);
    for capability in manifest.capabilities {
        let item = PyDict::new(py);
        item.set_item("domain", capability.domain)?;
        item.set_item("owner", capability.owner)?;
        item.set_item(
            "information_access",
            public_legality_bootstrap_information_access_name(capability.information_access),
        )?;
        item.set_item("deterministic", capability.deterministic)?;
        item.set_item(
            "coverage",
            public_legality_bootstrap_coverage_name(capability.coverage),
        )?;
        item.set_item("fallback", capability.fallback)?;
        item.set_item("limitation_ids", capability.limitation_ids.to_vec())?;
        capabilities.append(item)?;
    }
    result.set_item("capabilities", capabilities)?;

    let limitations = PyList::empty(py);
    for limitation in manifest.limitations {
        let item = PyDict::new(py);
        item.set_item("id", limitation.id)?;
        item.set_item("title", limitation.title)?;
        item.set_item("consequence", limitation.consequence)?;
        limitations.append(item)?;
    }
    result.set_item("limitations", limitations)?;
    Ok(result)
}

fn public_legality_bootstrap_information_access_name(
    access: PublicBaselineInformationAccessV1,
) -> &'static str {
    match access {
        PublicBaselineInformationAccessV1::PublicSemanticViewsOnly => "public_semantic_views_only",
    }
}

fn public_legality_bootstrap_coverage_name(coverage: PublicBaselineCoverageV1) -> &'static str {
    match coverage {
        PublicBaselineCoverageV1::SpecializedHeuristic => "specialized_heuristic",
        PublicBaselineCoverageV1::GenericHeuristic => "generic_heuristic",
        PublicBaselineCoverageV1::MechanicalFallback => "mechanical_fallback",
    }
}

fn numeric_schema_dict<'py>(
    py: Python<'py>,
    entries: &[(&str, i64)],
) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    for (name, value) in entries {
        result.set_item(*name, *value)?;
    }
    Ok(result)
}

fn value_error(error: impl ToString) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn runtime_error(error: impl ToString) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add(
        "NATIVE_BUILD_PROFILE",
        if cfg!(debug_assertions) {
            "dev"
        } else {
            "release"
        },
    )?;
    module.add_class::<guided::GuidedCombatBatch>()?;
    module.add_function(wrap_pyfunction!(guided::combat_entity_batch, module)?)?;
    module.add_class::<policy::PolicySearchBatch>()?;
    module.add_function(wrap_pyfunction!(policy::policy_demonstration, module)?)?;
    module.add_class::<distance::DistanceCombatSearch>()?;
    module.add_function(wrap_pyfunction!(distance::distance_route_data, module)?)?;
    module.add_class::<segment::GuidedSegmentTask>()?;
    module.add_class::<CombatLearningBatchEnv>()?;
    module.add_class::<CombatLearningDecisionProgressV1>()?;
    module.add_class::<CombatLearningRecoveryRoot>()?;
    module.add_class::<PyCompleteTurnExactEnvV1>()?;
    module.add_class::<PyCompleteTurnMaterializedOutcomeV1>()?;
    module.add_class::<PyCombatLearningRootContextV1>()?;
    module.add_class::<PyCombatLearningRootAuditV1>()?;
    module.add_class::<PyLearningPublicRunContextV1>()?;
    module.add_class::<PyLearningPublicDecisionSnapshotV1>()?;
    module.add_class::<LearningBatchEnv>()?;
    module.add_class::<FixedCombatBranchChoice>()?;
    module.add_class::<FixedCombatResearch>()?;
    module.add_class::<LearningCheckpointBatch>()?;
    module.add_class::<LearningSlotCheckpoint>()?;
    module.add_function(wrap_pyfunction!(
        complete_turn_demonstration_semantic_batch,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        replay_verified_winning_complete_turn_line,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(semantic_schema, module)?)?;
    module.add_function(wrap_pyfunction!(
        public_legality_bootstrap_manifest,
        module
    )?)?;
    module.add("PHASE_STRATEGIC_ROOT", PHASE_STRATEGIC_ROOT)?;
    module.add("PHASE_COMBAT_ROOT", PHASE_COMBAT_ROOT)?;
    module.add("PHASE_SELECTION", PHASE_SELECTION)?;
    module.add("RUN_BOUNDARY_STRATEGIC", RUN_BOUNDARY_STRATEGIC)?;
    module.add("RUN_BOUNDARY_COMBAT", RUN_BOUNDARY_COMBAT)?;
    module.add("RUN_BOUNDARY_TERMINAL", RUN_BOUNDARY_TERMINAL)?;
    module.add("RUN_BOUNDARY_UNSUPPORTED", RUN_BOUNDARY_UNSUPPORTED)?;
    module.add("COMBAT_TERMINAL_WIN", COMBAT_TERMINAL_WIN)?;
    module.add("COMBAT_TERMINAL_LOSS", COMBAT_TERMINAL_LOSS)?;
    module.add("COMBAT_TERMINAL_UNRESOLVED", COMBAT_TERMINAL_UNRESOLVED)?;
    module.add("SEMANTIC_SCHEMA_VERSION", SEMANTIC_SCHEMA_VERSION)?;
    module.add(
        "PUBLIC_LEGALITY_BOOTSTRAP_NAME",
        PUBLIC_DETERMINISTIC_BASELINE_NAME_V1,
    )?;
    module.add(
        "PUBLIC_LEGALITY_BOOTSTRAP_REVISION",
        PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
    )?;
    module.add(
        "SEMANTIC_NOT_ENCODED",
        SemanticCompleteness::NotEncoded as u8,
    )?;
    module.add("SEMANTIC_COMPLETE", SemanticCompleteness::Complete as u8)?;
    module.add("SEMANTIC_NO_CANDIDATE_TOKEN", NO_CANDIDATE_TOKEN)?;
    module.add("SEMANTIC_TOKEN_CANDIDATE", TokenKind::Candidate as u16)?;
    module.add(
        "SEMANTIC_RELATION_OBSERVATION_HAS_CANDIDATE",
        RelationKind::ObservationHasCandidate as u16,
    )?;
    module.add(
        "SEMANTIC_RELATION_CANDIDATE_TARGETS",
        RelationKind::CandidateTargets as u16,
    )?;
    Ok(())
}

fn learning_ready_paths(states: &[BridgeSlotState]) -> PyResult<Vec<LearningDecisionPathV3>> {
    if !bridge_states_ready(states) {
        return Err(PyValueError::new_err(
            "all active slots must finish root and selection decisions before step",
        ));
    }
    states
        .iter()
        .enumerate()
        .filter_map(|(slot_index, state)| match state {
            BridgeSlotState::Ready {
                decision_ordinals,
                source_token,
                ..
            } => Some((slot_index, decision_ordinals, source_token)),
            BridgeSlotState::Terminal => None,
            BridgeSlotState::Root | BridgeSlotState::Selection { .. } => unreachable!(),
        })
        .map(|(slot_index, decision_ordinals, source_token)| {
            Ok(LearningDecisionPathV3 {
                source: source_token.clone().ok_or_else(|| {
                    PyRuntimeError::new_err(format!(
                        "run slot {slot_index} has no frozen V3 decision token"
                    ))
                })?,
                ordinals: decision_ordinals.clone(),
            })
        })
        .collect::<PyResult<Vec<_>>>()
}

fn learning_transitions_to_python<'py>(
    py: Python<'py>,
    transitions: &[LearningTransitionV3],
) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item(
        "slot_indices",
        usize_array(
            py,
            transitions
                .iter()
                .map(|transition| transition.slot_index)
                .collect(),
        ),
    )?;
    result.set_item(
        "reward",
        PyArray1::from_vec(
            py,
            transitions
                .iter()
                .map(|transition| transition.reward)
                .collect(),
        ),
    )?;
    result.set_item(
        "terminated",
        PyArray1::from_vec(
            py,
            transitions
                .iter()
                .map(|transition| transition.next.is_none())
                .collect(),
        ),
    )?;
    result.set_item(
        "source_decision_token_ids",
        transitions
            .iter()
            .map(|transition| transition.source.token_id.clone())
            .collect::<Vec<_>>(),
    )?;
    let mut action_ordinal_row_splits = Vec::with_capacity(transitions.len() + 1);
    let mut action_ordinals = Vec::new();
    action_ordinal_row_splits.push(0);
    for transition in transitions {
        action_ordinals.extend_from_slice(&transition.action_ordinals);
        action_ordinal_row_splits.push(action_ordinals.len());
    }
    result.set_item(
        "action_ordinal_row_splits",
        usize_array(py, action_ordinal_row_splits),
    )?;
    result.set_item("action_ordinals", usize_array(py, action_ordinals))?;
    result.set_item(
        "next_decision_token_available",
        PyArray1::from_vec(
            py,
            transitions
                .iter()
                .map(|transition| transition.next.is_some())
                .collect(),
        ),
    )?;
    result.set_item(
        "next_decision_token_ids",
        transitions
            .iter()
            .map(|transition| {
                transition
                    .next
                    .as_ref()
                    .map(|token| token.token_id.clone())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>(),
    )?;
    result.set_item(
        "next_public_snapshot_ids",
        transitions
            .iter()
            .map(|transition| {
                transition
                    .next
                    .as_ref()
                    .map(|token| token.public_snapshot_id.clone())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>(),
    )?;
    result.set_item(
        "next_decision_steps",
        PyArray1::from_vec(
            py,
            transitions
                .iter()
                .map(|transition| {
                    transition
                        .next
                        .as_ref()
                        .map(|token| token.decision_step)
                        .unwrap_or_default()
                })
                .collect(),
        ),
    )?;
    let terminal_slots = transitions
        .iter()
        .filter_map(|transition| {
            transition
                .terminal_outcome
                .as_ref()
                .map(|outcome| (transition, outcome))
        })
        .collect::<Vec<_>>();
    result.set_item(
        "terminal_slot_indices",
        usize_array(
            py,
            terminal_slots
                .iter()
                .map(|(slot, _)| slot.slot_index)
                .collect(),
        ),
    )?;
    result.set_item(
        "terminal_reward",
        PyArray1::from_vec(
            py,
            terminal_slots.iter().map(|(slot, _)| slot.reward).collect(),
        ),
    )?;
    result.set_item(
        "terminal_act",
        PyArray1::from_vec(
            py,
            terminal_slots
                .iter()
                .map(|(_, outcome)| outcome.terminal_act)
                .collect(),
        ),
    )?;
    for (key, values) in [
        (
            "terminal_floor",
            terminal_slots
                .iter()
                .map(|(_, outcome)| outcome.terminal_floor)
                .collect(),
        ),
        (
            "terminal_hp",
            terminal_slots
                .iter()
                .map(|(_, outcome)| outcome.terminal_hp)
                .collect(),
        ),
        (
            "terminal_max_hp",
            terminal_slots
                .iter()
                .map(|(_, outcome)| outcome.terminal_max_hp)
                .collect(),
        ),
        (
            "terminal_gold",
            terminal_slots
                .iter()
                .map(|(_, outcome)| outcome.terminal_gold)
                .collect(),
        ),
    ] {
        result.set_item(key, PyArray1::from_vec(py, values))?;
    }
    Ok(result)
}
