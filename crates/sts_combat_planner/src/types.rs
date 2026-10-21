use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sts_core::ai::combat_state_key::{
    combat_exact_state_hash_v2, combat_exact_state_key, combat_exact_state_key_hash_v2,
    CombatExactStateKey,
};
use sts_core::sim::combat::{CombatPosition, CombatTerminal};
use sts_core::state::core::{ClientInput, EngineState};

use crate::trajectory_query::CombatTrajectoryProgressV1;

#[derive(Clone, Debug)]
pub struct CombatDecisionRoot {
    position: CombatPosition,
    exact_state_identity: ReplaySuccessorHash,
    turn_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatDecisionRootError {
    NotStablePlayerTurn,
    AlreadyTerminal,
}

impl CombatDecisionRoot {
    pub fn new(position: CombatPosition) -> Result<Self, CombatDecisionRootError> {
        Self::validate(&position)?;
        let exact_key = Arc::new(combat_exact_state_key(&position.engine, &position.combat));
        Ok(Self {
            exact_state_identity: ReplaySuccessorHash::from_exact_key(exact_key),
            turn_count: position.combat.turn.turn_count,
            position,
        })
    }

    pub(crate) fn with_exact_state_identity(
        position: CombatPosition,
        exact_state_identity: ReplaySuccessorHash,
    ) -> Result<Self, CombatDecisionRootError> {
        Self::validate(&position)?;
        Ok(Self {
            exact_state_identity,
            turn_count: position.combat.turn.turn_count,
            position,
        })
    }

    fn validate(position: &CombatPosition) -> Result<(), CombatDecisionRootError> {
        if !matches!(
            position.engine,
            EngineState::CombatPlayerTurn | EngineState::PendingChoice(_)
        ) {
            return Err(CombatDecisionRootError::NotStablePlayerTurn);
        }
        if sts_core::sim::combat::combat_terminal(&position.engine, &position.combat)
            != CombatTerminal::Unresolved
        {
            return Err(CombatDecisionRootError::AlreadyTerminal);
        }
        Ok(())
    }

    pub fn position(&self) -> &CombatPosition {
        &self.position
    }

    pub fn exact_state_hash(&self) -> &str {
        self.exact_state_identity.as_str()
    }

    pub(crate) fn exact_state_identity(&self) -> &ReplaySuccessorHash {
        &self.exact_state_identity
    }

    pub(crate) fn exact_state_key(&self) -> Option<&Arc<CombatExactStateKey>> {
        self.exact_state_identity.exact_key()
    }

    pub fn turn_count(&self) -> u32 {
        self.turn_count
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatPlanningCounters {
    pub generation_work: usize,
    pub engine_steps: usize,
}

impl CombatPlanningCounters {
    pub(crate) fn saturating_add(self, other: Self) -> Self {
        Self {
            generation_work: self.generation_work.saturating_add(other.generation_work),
            engine_steps: self.engine_steps.saturating_add(other.engine_steps),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CombatPlanningQuantum {
    pub additional_generation_work: usize,
    pub additional_engine_steps: usize,
    pub deadline: Option<Instant>,
}

impl CombatPlanningQuantum {
    pub fn deterministic(generation_work: usize, engine_steps: usize) -> Self {
        Self {
            additional_generation_work: generation_work,
            additional_engine_steps: engine_steps,
            deadline: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnOptionGeneratorConfig {
    /// A transition starts only after this whole allowance is reserved. That
    /// makes splitting a deterministic budget between quanta replay-free.
    pub max_engine_steps_per_transition: usize,
    /// Probability mass reserved for uniform exploration after expert weights
    /// are normalized. One million means a fully uniform policy.
    pub uniform_exploration_ppm: u32,
    /// Whether potion-spending inputs belong to this generator's legal search
    /// surface. Use and discard both consume a run resource. This is used only
    /// for an explicit zero-potion resource phase; ordinary search keeps the
    /// complete legal surface.
    pub allow_potion_expenditure: bool,
    /// Whether explicit potion-discard inputs belong to the search surface.
    /// Semantic victory search normally disables them while an all-legal
    /// oracle may retain them for slot-generation or revive-priority cases.
    pub allow_potion_discard: bool,
    /// Optional bit mask restricting potion use and discard inputs to exact
    /// slot indices while preserving the unchanged combat root.
    ///
    /// `None` keeps the complete legal potion surface. `Some(0)` removes
    /// every explicit potion expenditure. Passive effects such as Fairy
    /// Potion remain simulator truth and must be audited from replay.
    pub allowed_potion_slots: Option<u64>,
}

impl Default for TurnOptionGeneratorConfig {
    fn default() -> Self {
        Self {
            max_engine_steps_per_transition: 512,
            uniform_exploration_ppm: 50_000,
            allow_potion_expenditure: true,
            allow_potion_discard: true,
            allowed_potion_slots: None,
        }
    }
}

/// Stable replay identity for one exact action successor.
///
/// Search can retain the already-built typed key and defer its comparatively
/// expensive durable debug digest until a replay or serialized artifact
/// actually asks for it. Deserialized witnesses remain ordinary eager hashes.
#[derive(Clone)]
pub struct ReplaySuccessorHash {
    cached: Arc<OnceLock<String>>,
    exact_key: Option<Arc<CombatExactStateKey>>,
}

impl ReplaySuccessorHash {
    pub(crate) fn from_exact_key(exact_key: Arc<CombatExactStateKey>) -> Self {
        Self {
            cached: Arc::new(OnceLock::new()),
            exact_key: Some(exact_key),
        }
    }

    pub fn as_str(&self) -> &str {
        self.cached.get_or_init(|| {
            self.exact_key
                .as_ref()
                .map(|key| combat_exact_state_key_hash_v2(key))
                .expect("a deferred replay hash retains its exact key")
        })
    }

    pub(crate) fn exact_key(&self) -> Option<&Arc<CombatExactStateKey>> {
        self.exact_key.as_ref()
    }
}

impl From<String> for ReplaySuccessorHash {
    fn from(hash: String) -> Self {
        let cached = OnceLock::new();
        cached
            .set(hash)
            .expect("a fresh replay hash cache is empty");
        Self {
            cached: Arc::new(cached),
            exact_key: None,
        }
    }
}

impl From<&str> for ReplaySuccessorHash {
    fn from(hash: &str) -> Self {
        hash.to_owned().into()
    }
}

impl fmt::Debug for ReplaySuccessorHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ReplaySuccessorHash")
            .field(&self.as_str())
            .finish()
    }
}

impl PartialEq for ReplaySuccessorHash {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for ReplaySuccessorHash {}

impl Serialize for ReplaySuccessorHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ReplaySuccessorHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TurnOptionAction {
    pub input: ClientInput,
    pub expected_successor_hash: ReplaySuccessorHash,
    pub engine_steps: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompleteTurnOptionBoundary {
    NextPlayerTurn,
    TerminalWin,
    TerminalLoss,
    Escape,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompleteTurnOptionSource {
    ExactEnumeration,
    EncounterPlanPrefix,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompleteTurnOption {
    root_exact_state_identity: ReplaySuccessorHash,
    actions: Vec<TurnOptionAction>,
    boundary: CompleteTurnOptionBoundary,
    source: CompleteTurnOptionSource,
    exact_successor_hash: ReplaySuccessorHash,
    exact_successor: CombatPosition,
    engine_steps: usize,
    negative_log_policy: f64,
    trajectory_progress: Option<CombatTrajectoryProgressV1>,
}

impl CompleteTurnOption {
    pub(crate) fn new(
        root_exact_state_identity: impl Into<ReplaySuccessorHash>,
        actions: Vec<TurnOptionAction>,
        boundary: CompleteTurnOptionBoundary,
        exact_successor: CombatPosition,
        negative_log_policy: f64,
    ) -> Self {
        Self::with_source(
            root_exact_state_identity,
            actions,
            boundary,
            CompleteTurnOptionSource::ExactEnumeration,
            exact_successor,
            negative_log_policy,
        )
    }

    pub(crate) fn from_encounter_plan_prefix(
        root_exact_state_identity: impl Into<ReplaySuccessorHash>,
        actions: Vec<TurnOptionAction>,
        boundary: CompleteTurnOptionBoundary,
        exact_successor: CombatPosition,
        negative_log_policy: f64,
    ) -> Self {
        Self::with_source(
            root_exact_state_identity,
            actions,
            boundary,
            CompleteTurnOptionSource::EncounterPlanPrefix,
            exact_successor,
            negative_log_policy,
        )
    }

    fn with_source(
        root_exact_state_identity: impl Into<ReplaySuccessorHash>,
        actions: Vec<TurnOptionAction>,
        boundary: CompleteTurnOptionBoundary,
        source: CompleteTurnOptionSource,
        exact_successor: CombatPosition,
        negative_log_policy: f64,
    ) -> Self {
        let engine_steps = actions.iter().map(|action| action.engine_steps).sum();
        // A complete turn boundary is reached by the final exact action, so
        // its replay hash is already the exact successor identity. Rebuilding
        // and hashing the full combat key here duplicates the hottest work in
        // action generation. The fallback only covers defensive construction
        // of an empty action list.
        let exact_successor_hash = actions
            .last()
            .map(|action| action.expected_successor_hash.clone())
            .unwrap_or_else(|| ReplaySuccessorHash::from(exact_hash(&exact_successor)));
        Self {
            root_exact_state_identity: root_exact_state_identity.into(),
            exact_successor_hash,
            actions,
            boundary,
            source,
            exact_successor,
            engine_steps,
            negative_log_policy,
            trajectory_progress: None,
        }
    }

    pub(crate) fn with_trajectory_progress(
        mut self,
        trajectory_progress: Option<CombatTrajectoryProgressV1>,
    ) -> Self {
        self.trajectory_progress = trajectory_progress;
        self
    }

    pub fn root_exact_state_hash(&self) -> &str {
        self.root_exact_state_identity.as_str()
    }

    pub fn actions(&self) -> &[TurnOptionAction] {
        &self.actions
    }

    pub fn boundary(&self) -> CompleteTurnOptionBoundary {
        self.boundary
    }

    pub(crate) fn source(&self) -> CompleteTurnOptionSource {
        self.source
    }

    pub fn exact_successor_hash(&self) -> &str {
        self.exact_successor_hash.as_str()
    }

    pub fn exact_successor(&self) -> &CombatPosition {
        &self.exact_successor
    }

    pub fn engine_steps(&self) -> usize {
        self.engine_steps
    }

    pub fn negative_log_policy(&self) -> f64 {
        self.negative_log_policy
    }

    pub(crate) fn trajectory_progress(&self) -> Option<&CombatTrajectoryProgressV1> {
        self.trajectory_progress.as_ref()
    }

    pub(crate) fn into_successor_parts(
        self,
    ) -> (
        ReplaySuccessorHash,
        CombatPosition,
        Vec<TurnOptionAction>,
        f64,
        Option<CombatTrajectoryProgressV1>,
    ) {
        (
            self.exact_successor_hash,
            self.exact_successor,
            self.actions,
            self.negative_log_policy,
            self.trajectory_progress,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationInterruption {
    GenerationWorkBudget,
    EngineStepBudget,
    Deadline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnOptionGenerationGapKind {
    UnsupportedStableBoundary,
    UnsupportedStructuredChoice,
    DisabledStructuredChoice,
    EmptyLegalActionSurface,
    GeneratedInputRejected,
    TransitionStepLimit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnOptionGenerationGap {
    pub kind: TurnOptionGenerationGapKind,
    pub exact_state_hash: String,
    pub action_depth: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnOptionGenerationStatus {
    Complete,
    PlanPrefixServiced,
    Partial(GenerationInterruption),
    PartialWithMechanicsGaps,
}

#[derive(Clone, Debug)]
pub struct TurnOptionGenerationReport {
    pub before: CombatPlanningCounters,
    pub after: CombatPlanningCounters,
    pub granted: CombatPlanningCounters,
    pub before_diagnostics: TurnOptionGenerationDiagnostics,
    pub after_diagnostics: TurnOptionGenerationDiagnostics,
    pub retained_work_items: usize,
    pub newly_completed_options: usize,
    pub total_completed_options: usize,
    pub gaps: Vec<TurnOptionGenerationGap>,
    pub status: TurnOptionGenerationStatus,
}

/// Non-budget accounting for generation-time state merging. These counters
/// describe work already performed; they never affect legality, priority, or
/// stopping conditions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TurnOptionGenerationDiagnostics {
    pub applied_action_transitions: usize,
    pub unique_successor_states: usize,
    pub duplicate_exact_successors: usize,
    pub completed_turn_options: usize,
    pub plan_prefix_attempts: usize,
    pub plan_prefix_completed: usize,
    pub plan_prefix_rejections: usize,
}

pub(crate) fn exact_hash(position: &CombatPosition) -> String {
    combat_exact_state_hash_v2(&position.engine, &position.combat)
}

pub(crate) fn supported_boundary(
    root: &CombatDecisionRoot,
    position: &CombatPosition,
    terminal: CombatTerminal,
) -> Option<CompleteTurnOptionBoundary> {
    if terminal == CombatTerminal::Win && position.combat.runtime.combat_smoked {
        return Some(CompleteTurnOptionBoundary::Escape);
    }
    match terminal {
        CombatTerminal::Win => Some(CompleteTurnOptionBoundary::TerminalWin),
        CombatTerminal::Loss => Some(CompleteTurnOptionBoundary::TerminalLoss),
        CombatTerminal::Unresolved
            if matches!(position.engine, EngineState::CombatPlayerTurn)
                && position.combat.turn.turn_count > root.turn_count() =>
        {
            Some(CompleteTurnOptionBoundary::NextPlayerTurn)
        }
        CombatTerminal::Unresolved => None,
    }
}
