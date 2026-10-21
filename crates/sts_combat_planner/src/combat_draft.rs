use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use sts_core::sim::combat::{CombatPosition, CombatStepper};
use sts_core::sim::combat_action_equivalence::{
    equivalent_combat_selection_input_v1, EquivalentCombatSelectionInputV1,
};

use crate::atomic_witness::{replay_atomic_actions, AtomicWitnessReplayError, ExactAtomicWitness};
use crate::policy::{uniform_policy, SharedCombatActionPolicy};
use crate::replay::{replay_turn_option, ReplayError, ReplayLimits};
use crate::types::{
    CombatDecisionRoot, CombatDecisionRootError, CombatPlanningCounters, CombatPlanningQuantum,
    CompleteTurnOption, CompleteTurnOptionBoundary, TurnOptionAction, TurnOptionGenerationReport,
    TurnOptionGeneratorConfig,
};
use crate::{SelectionIntent, SelectionIntentError, SelectionSource, TurnOptionGeneratorSession};
use sts_core::state::core::ClientInput;

/// A short option identity scoped to one retained draft frame.
///
/// The frame identity deliberately stays private. Interactive projections may
/// show `T0`, `T1`, and so on without exposing hashes, while a handle retained
/// across a move cannot silently select an option from a different turn.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CombatDraftOptionHandle {
    frame_id: CombatDraftFrameHandle,
    ordinal: usize,
}

/// Process-local identity of one retained turn, independent of branch depth.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CombatDraftFrameHandle(u64);

impl CombatDraftFrameHandle {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        Self(
            NEXT_ID
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .expect("combat draft frame identities exhausted"),
        )
    }
}

impl fmt::Display for CombatDraftFrameHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "F{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatDraftContinuation {
    pub frame: CombatDraftFrameHandle,
    pub available_option_count: usize,
    pub counters: CombatPlanningCounters,
    pub coverage: CombatDraftOptionCoverage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatDraftSelectionOutcome {
    pub handle: CombatDraftOptionHandle,
    /// Actual retained action; never replaced with its equivalent selection.
    pub selection: ClientInput,
    pub equivalent_selection: Option<EquivalentCombatSelectionInputV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatDraftSelectionOutcomePage {
    pub outcomes: Vec<CombatDraftSelectionOutcome>,
    pub total: usize,
}

/// Coverage of the current complete-turn option surface. This is independent
/// from both analyst selection and whole-combat victory evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatDraftOptionCoverage {
    Open { mechanics_gap_count: usize },
    Complete,
    MechanicsGap { gap_count: usize },
}

impl CombatDraftOptionHandle {
    pub fn ordinal(self) -> usize {
        self.ordinal
    }
}

impl fmt::Display for CombatDraftOptionHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "T{}", self.ordinal)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CombatDraftError {
    DraftAlreadyTerminal,
    NoPreviousTurn,
    UnknownOption { ordinal: usize },
    StaleOptionHandle,
    SelectionIntent(SelectionIntentError),
    TurnReplay(ReplayError),
    InvalidNextTurnRoot(CombatDecisionRootError),
    VictoryReplay(AtomicWitnessReplayError),
    VictoryFinalPositionMismatch,
    VictoryIsNotExactWin,
    PrefixReplay(AtomicWitnessReplayError),
    PrefixIsNotTurnBoundary,
}

struct CombatDraftFrame {
    id: CombatDraftFrameHandle,
    generator: TurnOptionGeneratorSession,
    children: BTreeMap<usize, usize>,
}

impl CombatDraftFrame {
    fn new(generator: TurnOptionGeneratorSession) -> Self {
        Self {
            id: CombatDraftFrameHandle::new(),
            generator,
            children: BTreeMap::new(),
        }
    }

    fn coverage(&self) -> CombatDraftOptionCoverage {
        let gap_count = self.generator.gaps().len();
        if !self.generator.is_finished() {
            CombatDraftOptionCoverage::Open {
                mechanics_gap_count: gap_count,
            }
        } else if gap_count == 0 {
            CombatDraftOptionCoverage::Complete
        } else {
            CombatDraftOptionCoverage::MechanicsGap { gap_count }
        }
    }
}

#[derive(Clone, Debug)]
struct CombatDraftTerminalState {
    boundary: CompleteTurnOptionBoundary,
    position: CombatPosition,
}

/// An analyst-owned, exact, turn-at-a-time combat draft.
///
/// Each frame owns one resumable complete-turn generator. Choosing an option
/// exact-replays it before advancing. Undo returns to the parent while keeping
/// the child generator resident; choosing that same option resumes its work.
/// Search policy may order candidates but never chooses a turn for the
/// analyst or grants victory authority.
pub struct CombatDraftSession {
    original_root: CombatDecisionRoot,
    fixed_prefix: Vec<TurnOptionAction>,
    config: TurnOptionGeneratorConfig,
    policy: SharedCombatActionPolicy,
    frames: Vec<CombatDraftFrame>,
    active_path: Vec<usize>,
    selected_turns: Vec<CompleteTurnOption>,
    terminal: Option<CombatDraftTerminalState>,
}

impl CombatDraftSession {
    pub fn new(root: CombatDecisionRoot, config: TurnOptionGeneratorConfig) -> Self {
        Self::with_policy(root, config, uniform_policy())
    }

    pub fn with_policy(
        root: CombatDecisionRoot,
        config: TurnOptionGeneratorConfig,
        policy: SharedCombatActionPolicy,
    ) -> Self {
        let generator =
            TurnOptionGeneratorSession::with_policy(root.clone(), config, policy.clone());
        Self {
            original_root: root,
            fixed_prefix: Vec::new(),
            config,
            policy,
            frames: vec![CombatDraftFrame::new(generator)],
            active_path: vec![0],
            selected_turns: Vec::new(),
            terminal: None,
        }
    }

    /// Start editing after an exact retained prefix. The prefix is immutable;
    /// undo stops here, while victory export still replays from the original root.
    /// This constructor performs no candidate generation or strategic selection.
    pub fn from_exact_prefix(
        root: CombatDecisionRoot,
        config: TurnOptionGeneratorConfig,
        prefix: Vec<TurnOptionAction>,
        stepper: &dyn CombatStepper,
    ) -> Result<Self, CombatDraftError> {
        if prefix.is_empty() {
            return Ok(Self::new(root, config));
        }
        let last = prefix.len() - 1;
        let (before_last, _) = replay_atomic_actions(
            stepper,
            root.position(),
            &prefix[..last],
            config.max_engine_steps_per_transition.max(1),
        )
        .map_err(CombatDraftError::PrefixReplay)?;
        let boundary_root =
            CombatDecisionRoot::new(before_last).map_err(CombatDraftError::InvalidNextTurnRoot)?;
        let (position, _) = replay_atomic_actions(
            stepper,
            boundary_root.position(),
            &prefix[last..],
            config.max_engine_steps_per_transition.max(1),
        )
        .map_err(CombatDraftError::PrefixReplay)?;
        if crate::types::supported_boundary(&boundary_root, &position, stepper.terminal(&position))
            != Some(CompleteTurnOptionBoundary::NextPlayerTurn)
        {
            return Err(CombatDraftError::PrefixIsNotTurnBoundary);
        }
        let next =
            CombatDecisionRoot::new(position).map_err(CombatDraftError::InvalidNextTurnRoot)?;
        if next.turn_count() <= root.turn_count() {
            return Err(CombatDraftError::PrefixIsNotTurnBoundary);
        }
        let mut draft = Self::new(next, config);
        draft.original_root = root;
        draft.fixed_prefix = prefix;
        Ok(draft)
    }

    pub fn current_root(&self) -> Option<&CombatDecisionRoot> {
        self.terminal
            .is_none()
            .then(|| self.current_frame().generator.root())
    }

    pub fn selected_turn_count(&self) -> usize {
        (self.frames[0].generator.root().turn_count() - self.original_root.turn_count()) as usize
            + self.selected_turns.len()
    }

    pub fn current_frame_handle(&self) -> Option<CombatDraftFrameHandle> {
        self.terminal.is_none().then(|| self.current_frame().id)
    }

    pub fn terminal_boundary(&self) -> Option<CompleteTurnOptionBoundary> {
        self.terminal.as_ref().map(|terminal| terminal.boundary)
    }

    pub fn terminal_position(&self) -> Option<&CombatPosition> {
        self.terminal.as_ref().map(|terminal| &terminal.position)
    }

    pub fn current_generation_counters(&self) -> Option<CombatPlanningCounters> {
        self.current_root()
            .map(|_| self.current_frame().generator.counters())
    }

    pub fn root_selection_intent(&self) -> Option<SelectionIntent> {
        self.current_root()
            .map(|_| self.current_frame().generator.root_selection_intent())
    }

    pub fn selection_intent(&self) -> Option<SelectionIntent> {
        self.current_root()
            .map(|_| self.current_frame().generator.selection_intent())
    }

    pub fn selection_sources(
        &self,
        stepper: &dyn CombatStepper,
        offset: usize,
        limit: usize,
    ) -> Result<crate::SelectionSourcePage, CombatDraftError> {
        self.current_root()
            .ok_or(CombatDraftError::DraftAlreadyTerminal)?;
        Ok(self
            .current_frame()
            .generator
            .selection_sources(stepper, offset, limit))
    }

    pub fn selection_continuation_work(
        &self,
        source: &SelectionSource,
    ) -> Result<usize, CombatDraftError> {
        self.current_root()
            .ok_or(CombatDraftError::DraftAlreadyTerminal)?;
        self.current_frame()
            .generator
            .selection_continuation_work(source)
            .map_err(CombatDraftError::SelectionIntent)
    }

    /// Read already-materialized complete turns through this exact selection
    /// source. Preferred selections come first, preserving generation order
    /// within each group. Handles and witnesses do not change; no replay occurs.
    pub fn selection_outcomes(
        &self,
        source: &SelectionSource,
        offset: usize,
        limit: usize,
    ) -> Result<CombatDraftSelectionOutcomePage, CombatDraftError> {
        self.validate_selection_source(source)?;
        let frame = self.current_frame();
        let intent = frame.generator.selection_intent();
        let members = if intent.source == *source {
            intent.members.as_slice()
        } else {
            &[]
        };
        let mut total = 0;
        let mut outcomes = Vec::new();
        for preferred in [true, false] {
            for (ordinal, option) in frame.generator.completed_options().iter().enumerate() {
                let Some(selection) = source.selected_input_in(option.actions()) else {
                    continue;
                };
                if source.matches_preference(selection, members) != preferred {
                    continue;
                }
                if total >= offset && outcomes.len() < limit.min(16) {
                    outcomes.push(CombatDraftSelectionOutcome {
                        handle: CombatDraftOptionHandle {
                            frame_id: frame.id,
                            ordinal,
                        },
                        selection: selection.clone(),
                        equivalent_selection: equivalent_combat_selection_input_v1(
                            &source.position().engine,
                            selection,
                        ),
                    });
                }
                total += 1;
            }
        }
        Ok(CombatDraftSelectionOutcomePage { outcomes, total })
    }

    pub fn validate_selection_source(
        &self,
        source: &SelectionSource,
    ) -> Result<(), CombatDraftError> {
        self.current_root()
            .ok_or(CombatDraftError::DraftAlreadyTerminal)?;
        self.current_frame()
            .generator
            .validate_selection_source(source)
            .map_err(CombatDraftError::SelectionIntent)
    }

    pub fn selection_surface(
        &self,
        source: &SelectionSource,
        stepper: &dyn CombatStepper,
    ) -> Result<sts_core::sim::combat_action_surface::CombatLegalActionSurfaceV2, CombatDraftError>
    {
        self.validate_selection_source(source)?;
        let mut surface =
            crate::automatic_action_surface::automatic_action_surface(stepper, source.position());
        surface.atomic_actions.clear();
        surface.indexed_choice = None;
        Ok(surface)
    }

    pub fn replace_selection_intent(
        &mut self,
        source: &SelectionSource,
        members: &[ClientInput],
        stepper: &dyn CombatStepper,
    ) -> Result<(), CombatDraftError> {
        self.validate_selection_source(source)?;
        self.current_frame_mut()
            .generator
            .replace_selection_intent(source, members, stepper)
            .map_err(CombatDraftError::SelectionIntent)
    }

    /// An intention belongs to this exact frame root. Undo retains it with
    /// the parent generator; choosing a turn does not copy it to the child.
    pub fn replace_root_selection_intent(
        &mut self,
        source: &SelectionSource,
        members: &[ClientInput],
        stepper: &dyn CombatStepper,
    ) -> Result<(), CombatDraftError> {
        if self.terminal.is_some() {
            return Err(CombatDraftError::DraftAlreadyTerminal);
        }
        self.current_frame_mut()
            .generator
            .replace_root_selection_intent(source, members, stepper)
            .map_err(CombatDraftError::SelectionIntent)
    }

    pub fn current_option_coverage(&self) -> Option<CombatDraftOptionCoverage> {
        self.terminal
            .is_none()
            .then(|| self.current_frame().coverage())
    }

    /// Read only the already-explored immediate child of this exact option.
    pub fn option_continuation(
        &self,
        handle: CombatDraftOptionHandle,
    ) -> Result<Option<CombatDraftContinuation>, CombatDraftError> {
        self.option(handle)?;
        Ok(self
            .current_frame()
            .children
            .get(&handle.ordinal)
            .map(|index| {
                let child = &self.frames[*index];
                CombatDraftContinuation {
                    frame: child.id,
                    available_option_count: child.generator.completed_options().len(),
                    counters: child.generator.counters(),
                    coverage: child.coverage(),
                }
            }))
    }

    /// Advances only the generator for the current player-turn frame.
    ///
    /// The supplied quantum is forwarded exactly once. This method never
    /// loops until an option, exhaustion, or a wall-clock timeout appears.
    pub fn advance_options(
        &mut self,
        stepper: &dyn CombatStepper,
        quantum: CombatPlanningQuantum,
    ) -> Result<TurnOptionGenerationReport, CombatDraftError> {
        if self.terminal.is_some() {
            return Err(CombatDraftError::DraftAlreadyTerminal);
        }
        Ok(self.current_frame_mut().generator.advance(stepper, quantum))
    }

    pub fn option_handles(&self) -> Vec<CombatDraftOptionHandle> {
        if self.terminal.is_some() {
            return Vec::new();
        }
        let frame = self.current_frame();
        (0..frame.generator.completed_options().len())
            .map(|ordinal| CombatDraftOptionHandle {
                frame_id: frame.id,
                ordinal,
            })
            .collect()
    }

    pub fn option_handle(
        &self,
        ordinal: usize,
    ) -> Result<CombatDraftOptionHandle, CombatDraftError> {
        if self.terminal.is_some() {
            return Err(CombatDraftError::DraftAlreadyTerminal);
        }
        let frame = self.current_frame();
        if ordinal >= frame.generator.completed_options().len() {
            return Err(CombatDraftError::UnknownOption { ordinal });
        }
        Ok(CombatDraftOptionHandle {
            frame_id: frame.id,
            ordinal,
        })
    }

    pub fn option(
        &self,
        handle: CombatDraftOptionHandle,
    ) -> Result<&CompleteTurnOption, CombatDraftError> {
        if self.terminal.is_some() {
            return Err(CombatDraftError::DraftAlreadyTerminal);
        }
        let frame = self.current_frame();
        if handle.frame_id != frame.id {
            return Err(CombatDraftError::StaleOptionHandle);
        }
        frame
            .generator
            .completed_options()
            .get(handle.ordinal)
            .ok_or(CombatDraftError::UnknownOption {
                ordinal: handle.ordinal,
            })
    }

    /// Selects one already-generated complete turn after exact replay.
    pub fn choose(
        &mut self,
        handle: CombatDraftOptionHandle,
        stepper: &dyn CombatStepper,
    ) -> Result<CompleteTurnOptionBoundary, CombatDraftError> {
        let option = self.option(handle)?.clone();
        let replay = replay_turn_option(
            self.current_frame().generator.root(),
            &option,
            stepper,
            ReplayLimits::deterministic(option.engine_steps()),
        )
        .map_err(CombatDraftError::TurnReplay)?;

        match replay.boundary {
            CompleteTurnOptionBoundary::NextPlayerTurn => {
                let root = CombatDecisionRoot::new(replay.position)
                    .map_err(CombatDraftError::InvalidNextTurnRoot)?;
                let child = if let Some(index) = self.current_frame().children.get(&handle.ordinal)
                {
                    *index
                } else {
                    let generator = TurnOptionGeneratorSession::with_policy(
                        root,
                        self.config,
                        self.policy.clone(),
                    );
                    let index = self.frames.len();
                    self.frames.push(CombatDraftFrame::new(generator));
                    self.current_frame_mut()
                        .children
                        .insert(handle.ordinal, index);
                    index
                };
                self.selected_turns.push(option);
                self.active_path.push(child);
            }
            CompleteTurnOptionBoundary::TerminalWin
            | CompleteTurnOptionBoundary::TerminalLoss
            | CompleteTurnOptionBoundary::Escape => {
                self.selected_turns.push(option);
                self.terminal = Some(CombatDraftTerminalState {
                    boundary: replay.boundary,
                    position: replay.position,
                });
            }
        }
        Ok(replay.boundary)
    }

    /// Returns to the preceding analyst decision without regenerating it.
    pub fn undo(&mut self) -> Result<(), CombatDraftError> {
        if self.terminal.take().is_some() {
            self.selected_turns
                .pop()
                .expect("a terminal draft has one selected terminal turn");
            return Ok(());
        }
        if self.active_path.len() == 1 {
            return Err(CombatDraftError::NoPreviousTurn);
        }
        self.active_path.pop();
        self.selected_turns
            .pop()
            .expect("every child frame has one selected parent turn");
        Ok(())
    }

    pub fn exact_line(&self) -> Vec<&TurnOptionAction> {
        self.fixed_prefix
            .iter()
            .chain(self.selected_turns.iter().flat_map(|turn| turn.actions()))
            .collect()
    }

    /// Exports a witness only after replaying the complete selected line from
    /// the unchanged first root and proving an exact, non-Smoke-Bomb victory.
    pub fn victory_witness(
        &self,
        stepper: &dyn CombatStepper,
    ) -> Result<Option<ExactAtomicWitness>, CombatDraftError> {
        let Some(terminal) = self.terminal.as_ref() else {
            return Ok(None);
        };
        if terminal.boundary != CompleteTurnOptionBoundary::TerminalWin {
            return Ok(None);
        }

        let actions = self.exact_line().into_iter().cloned().collect::<Vec<_>>();
        let (final_position, replay_engine_steps) = replay_atomic_actions(
            stepper,
            self.original_root.position(),
            &actions,
            self.config.max_engine_steps_per_transition.max(1),
        )
        .map_err(CombatDraftError::VictoryReplay)?;
        if final_position != terminal.position {
            return Err(CombatDraftError::VictoryFinalPositionMismatch);
        }
        if !stepper.is_exact_victory(&final_position) {
            return Err(CombatDraftError::VictoryIsNotExactWin);
        }
        Ok(Some(ExactAtomicWitness {
            actions,
            final_position,
            negative_log_policy: self
                .selected_turns
                .iter()
                .map(CompleteTurnOption::negative_log_policy)
                .sum(),
            replay_engine_steps,
        }))
    }

    fn current_frame(&self) -> &CombatDraftFrame {
        &self.frames[*self
            .active_path
            .last()
            .expect("a draft always has an active frame")]
    }

    fn current_frame_mut(&mut self) -> &mut CombatDraftFrame {
        &mut self.frames[*self
            .active_path
            .last()
            .expect("a draft always has an active frame")]
    }
}
