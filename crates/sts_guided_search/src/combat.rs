use std::sync::Arc;
use std::time::Instant;

use sts_core::sim::combat::{
    stable_boundary, CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal,
    EngineCombatStepper,
};
use sts_core::state::core::{ClientInput, EngineState};

use crate::selection::{SelectionDomain, SelectionMember};
use crate::{
    Choice, NodeRef, SearchError, SearchSession, SearchSpace, Transition, TransitionInterruption,
};

#[cfg(test)]
mod prepared_tests;

#[derive(Clone, Debug, PartialEq)]
struct SelectionPrefix {
    family: usize,
    domain: Arc<SelectionDomain>,
    selected: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatSearchState {
    position: Arc<CombatPosition>,
    selection: Option<SelectionPrefix>,
    pub(crate) potion_uses: usize,
}

impl CombatSearchState {
    pub fn new(position: CombatPosition) -> Result<Self, String> {
        if !stable_boundary(&position.engine, &position.combat) {
            return Err("search position has not settled".into());
        }
        if EngineCombatStepper.terminal(&position) == CombatTerminal::Unresolved
            && !matches!(
                position.engine,
                EngineState::CombatPlayerTurn | EngineState::PendingChoice(_)
            )
        {
            return Err("search root is not a stable combat decision".into());
        }
        Ok(Self {
            position: Arc::new(position),
            selection: None,
            potion_uses: 0,
        })
    }

    pub fn position(&self) -> &CombatPosition {
        &self.position
    }
    /// Same immutable position allocation, with exact virtual choice and potion
    /// metadata. A fast positive identity check, never a fingerprint comparison.
    pub(crate) fn shares_exact_state(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.position, &other.position)
            && self.selection == other.selection
            && self.potion_uses == other.potion_uses
    }
    pub fn potion_uses(&self) -> usize {
        self.potion_uses
    }
    pub fn selection_members(&self) -> Option<&[SelectionMember]> {
        self.selection.as_ref().map(|s| s.domain.members.as_slice())
    }
    pub fn selected_members(&self) -> Option<&[usize]> {
        self.selection.as_ref().map(|s| s.selected.as_slice())
    }
    pub fn selection_family(&self) -> Option<usize> {
        self.selection.as_ref().map(|s| s.family)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CombatBranch {
    Input(ClientInput),
    BeginSelection(usize),
    SelectMember(usize),
    FinishSelection,
}

/// Task constraints are independent of the search allowance. None admits every
/// legal potion use; Some(n) caps actual uses including generated replacements.
#[derive(Clone, Debug, Default)]
pub struct CombatSearchSpace {
    max_potion_uses: Option<usize>,
}

/// An immutable source and its enumerated legal branches. Callers can reorder
/// indices but cannot attach this permission to another state or new branch.
pub(crate) struct PreparedCombatChoices {
    space: CombatSearchSpace,
    source: CombatSearchState,
    branches: Vec<CombatBranch>,
}

impl PreparedCombatChoices {
    pub(crate) fn branches(&self) -> &[CombatBranch] {
        &self.branches
    }

    /// Reconstruct a real input's grammar path at this immutable source. The
    /// caller still prepares and admits each subsequent virtual decision.
    pub(crate) fn input_path(&self, input: &ClientInput) -> Result<Vec<CombatBranch>, String> {
        if self.source.selection.is_some() {
            return Err("input path needs an unassembled source".into());
        }
        let surface = EngineCombatStepper.legal_action_surface(self.source.position());
        let mut found = None;
        for branch in &self.branches {
            let candidate = match branch {
                CombatBranch::Input(candidate) if candidate == input => Some(vec![branch.clone()]),
                CombatBranch::BeginSelection(family) => {
                    let domain = SelectionDomain::new(&surface.selection_families[*family])?;
                    domain.resolve(input).map(|members| {
                        let mut path = vec![branch.clone()];
                        path.extend(members.into_iter().map(CombatBranch::SelectMember));
                        path.push(CombatBranch::FinishSelection);
                        path
                    })
                }
                _ => None,
            };
            if let Some(path) = candidate {
                if found.replace(path).is_some() {
                    return Err("ambiguous input path at recorded source".into());
                }
            }
        }
        found.ok_or_else(|| "recorded input has no legal grammar path at source".into())
    }

    pub(crate) fn input(&self, index: usize) -> Result<Option<ClientInput>, String> {
        let branch = self
            .branches
            .get(index)
            .ok_or("invalid prepared choice index")?;
        self.space.input(&self.source, branch)
    }

    pub(crate) fn transition(
        &self,
        index: usize,
        limits: CombatStepLimits,
    ) -> Result<Transition<CombatSearchState>, String> {
        let branch = self
            .branches
            .get(index)
            .ok_or("invalid prepared choice index")?;
        self.space
            .transition_enumerated(&self.source, branch, limits)
    }
}

impl CombatSearchSpace {
    pub(crate) fn prepare_choices(
        &self,
        source: &CombatSearchState,
    ) -> Result<PreparedCombatChoices, String> {
        Ok(PreparedCombatChoices {
            space: self.clone(),
            source: source.clone(),
            branches: if self.terminal(source).is_some() {
                Vec::new()
            } else {
                self.enumerate_decision_choices(source, false)?
                    .into_iter()
                    .map(|c| c.action)
                    .collect()
            },
        })
    }

    pub fn with_potion_limit(max_uses: usize) -> Self {
        Self {
            max_potion_uses: Some(max_uses),
        }
    }

    pub fn max_potion_uses(&self) -> Option<usize> {
        self.max_potion_uses
    }

    // Called only after binding a legal branch. Kept separate from real input
    // execution: assembling a selection does not advance the game.
    pub(crate) fn virtual_successor(
        &self,
        state: &CombatSearchState,
        branch: &CombatBranch,
    ) -> Result<CombatSearchState, String> {
        let mut next = state.clone();
        match branch {
            CombatBranch::BeginSelection(index) => {
                let surface = EngineCombatStepper.legal_action_surface(&state.position);
                let domain = SelectionDomain::new(&surface.selection_families[*index])?;
                next.selection = Some(SelectionPrefix {
                    family: *index,
                    domain: Arc::new(domain),
                    selected: Vec::new(),
                });
            }
            CombatBranch::SelectMember(index) => next
                .selection
                .as_mut()
                .ok_or("missing selection source")?
                .selected
                .push(*index),
            _ => return Err("unexpected virtual choice".into()),
        }
        Ok(next)
    }

    pub(crate) fn input(
        &self,
        state: &CombatSearchState,
        branch: &CombatBranch,
    ) -> Result<Option<ClientInput>, String> {
        match branch {
            CombatBranch::Input(input) if state.selection.is_none() => Ok(Some(input.clone())),
            CombatBranch::FinishSelection => state
                .selection
                .as_ref()
                .ok_or_else(|| "missing selection source".into())
                .and_then(|s| s.domain.compile(&s.selected))
                .map(Some),
            CombatBranch::BeginSelection(_) | CombatBranch::SelectMember(_) => Ok(None),
            _ => Err("branch does not belong to this input boundary".into()),
        }
    }
}

// Run control still needs forced escape cleanup after a combat objective closes.
impl CombatSearchSpace {
    pub(crate) fn decision_choices(
        &self,
        state: &CombatSearchState,
    ) -> Result<Vec<Choice<CombatBranch>>, String> {
        self.enumerate_decision_choices(state, true)
    }

    // Legality and compilation do not require the legacy complete-input mass.
    // Existing weighted callers retain that distribution; prepared callers only
    // need the same linear-size set of legal branches.
    fn enumerate_decision_choices(
        &self,
        state: &CombatSearchState,
        weighted: bool,
    ) -> Result<Vec<Choice<CombatBranch>>, String> {
        if let Some(s) = &state.selection {
            let mut choices = Vec::new();
            if s.selected.len() >= s.domain.min {
                choices.push(Choice {
                    action: CombatBranch::FinishSelection,
                    exploration_weight: 1.0,
                });
            }
            if s.selected.len() < s.domain.max {
                let mass = if weighted {
                    s.domain.remaining_mass(s.selected.len() + 1)?
                } else {
                    1.0
                };
                for member in 0..s.domain.members.len() {
                    if !s.selected.contains(&member) {
                        choices.push(Choice {
                            action: CombatBranch::SelectMember(member),
                            exploration_weight: mass,
                        });
                    }
                }
            }
            return Ok(choices);
        }
        let surface = EngineCombatStepper.legal_action_surface(&state.position);
        let mut choices = surface
            .atomic_actions
            .into_iter()
            .filter(|input| {
                !matches!(input, ClientInput::UsePotion { .. })
                    || self
                        .max_potion_uses
                        .is_none_or(|max| state.potion_uses < max)
            })
            .map(|input| Choice {
                action: CombatBranch::Input(input),
                exploration_weight: 1.0,
            })
            .collect::<Vec<_>>();
        for (index, family) in surface.selection_families.iter().enumerate() {
            let domain = SelectionDomain::new(family)?;
            choices.push(Choice {
                action: CombatBranch::BeginSelection(index),
                exploration_weight: if weighted {
                    domain.remaining_mass(0)?
                } else {
                    1.0
                },
            });
        }
        Ok(choices)
    }
}

impl SearchSpace for CombatSearchSpace {
    type State = CombatSearchState;
    type Action = CombatBranch;

    fn terminal(&self, state: &CombatSearchState) -> Option<bool> {
        // Smoke has abandoned the defeat-enemies objective even while room
        // handoff remains pending. This is a task terminal, not a run exit.
        if state.position.combat.runtime.combat_smoked {
            return Some(false);
        }
        if self
            .max_potion_uses
            .is_some_and(|max| state.potion_uses > max)
        {
            return Some(false);
        }
        match EngineCombatStepper.terminal(&state.position) {
            CombatTerminal::Unresolved => None,
            CombatTerminal::Loss => Some(false),
            CombatTerminal::Win => Some(EngineCombatStepper.is_exact_victory(&state.position)),
        }
    }

    fn choices(&self, state: &CombatSearchState) -> Result<Vec<Choice<CombatBranch>>, String> {
        if self.terminal(state).is_some() {
            return Ok(Vec::new());
        }
        self.decision_choices(state)
    }

    fn transition(
        &self,
        state: &CombatSearchState,
        branch: &CombatBranch,
        limits: CombatStepLimits,
    ) -> Result<Transition<CombatSearchState>, String> {
        if limits.deadline.is_some_and(|d| Instant::now() >= d) {
            return Ok(Transition::Interrupted {
                engine_steps: 0,
                cause: TransitionInterruption::Deadline,
            });
        }
        if !self.choices(state)?.iter().any(|c| c.action == *branch) {
            return Err("branch is not a legal source-bound choice".into());
        }
        self.transition_enumerated(state, branch, limits)
    }
}

impl CombatSearchSpace {
    /// Only called after membership in this exact source's action enumeration.
    /// Prepared collections preserve that source across suspension; unbound
    /// SearchSpace transitions check membership on each call.
    fn transition_enumerated(
        &self,
        state: &CombatSearchState,
        branch: &CombatBranch,
        limits: CombatStepLimits,
    ) -> Result<Transition<CombatSearchState>, String> {
        if limits.deadline.is_some_and(|d| Instant::now() >= d) {
            return Ok(Transition::Interrupted {
                engine_steps: 0,
                cause: TransitionInterruption::Deadline,
            });
        }
        if let Some(input) = self.input(state, branch)? {
            // Atomic inputs already belong to this immutable source's legal
            // enumeration. Re-enumerating every card and target here repeats
            // preparation for each sibling. Assembled selections are new full
            // inputs, so validate those against the simulator before execution.
            if matches!(branch, CombatBranch::FinishSelection)
                && !EngineCombatStepper.is_legal_action(&state.position, &input)
            {
                return Err("assembled input rejected by simulator".into());
            }
            let potion_uses =
                state.potion_uses + usize::from(matches!(input, ClientInput::UsePotion { .. }));
            let result = EngineCombatStepper.apply_to_stable(&state.position, input, limits);
            if result.timed_out || result.truncated {
                return Ok(Transition::Interrupted {
                    engine_steps: result.engine_steps,
                    cause: if result.timed_out {
                        TransitionInterruption::Deadline
                    } else {
                        TransitionInterruption::EngineStepLimit
                    },
                });
            }
            let mut next = CombatSearchState::new(result.position)?;
            next.potion_uses = potion_uses;
            return Ok(Transition::Settled {
                state: next,
                engine_steps: result.engine_steps,
            });
        }
        Ok(Transition::Settled {
            state: self.virtual_successor(state, branch)?,
            engine_steps: 0,
        })
    }
}

/// Process-local, fixed-root combat replay. It is not a run-exit resource
/// materialization, a durable plan, Java parity, or authority to move a run head.
pub struct ReplayedCombatWin {
    root: CombatPosition,
    inputs: Vec<ClientInput>,
    successor: CombatPosition,
    engine_steps: usize,
}

/// One materialized input and its observed stable successor. Virtual selection
/// assembly is omitted. This read-only trace is not a replay certificate.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ObservedCombatInput {
    pub input: ClientInput,
    pub successor: CombatPosition,
}

impl ReplayedCombatWin {
    pub fn root(&self) -> &CombatPosition {
        &self.root
    }
    pub fn inputs(&self) -> &[ClientInput] {
        &self.inputs
    }
    pub fn successor(&self) -> &CombatPosition {
        &self.successor
    }
    pub fn engine_steps(&self) -> usize {
        self.engine_steps
    }
}

impl SearchSession<CombatSearchSpace> {
    pub fn observed_trace(&self, source: NodeRef) -> Result<Vec<ObservedCombatInput>, SearchError> {
        let mut steps = Vec::new();
        for (before, branch, after) in self.path(source)? {
            if let Some(input) = self
                .space
                .input(before, branch)
                .map_err(SearchError::Domain)?
            {
                steps.push(ObservedCombatInput {
                    input,
                    successor: after.position().clone(),
                });
            }
        }
        Ok(steps)
    }

    pub fn replay_win(
        &self,
        source: NodeRef,
        max_total_engine_steps: usize,
        max_steps_per_transition: usize,
        deadline: Option<Instant>,
    ) -> Result<ReplayedCombatWin, SearchError> {
        let view = self.view(source)?;
        if self.space.terminal(view.state) != Some(true) {
            return Err(SearchError::Domain(
                "candidate is not an observed victory".into(),
            ));
        }
        if max_steps_per_transition == 0 {
            return Err(SearchError::InvalidConfiguration);
        }
        let mut state = self.view(self.root())?.state.clone();
        let root = state.position().clone();
        let mut inputs = Vec::new();
        let mut used = 0;
        for (expected_before, branch, expected_after) in self.path(source)? {
            if &state != expected_before {
                return Err(SearchError::Domain("replay source changed".into()));
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return Err(SearchError::Domain("replay deadline reached".into()));
            }
            let input = self
                .space
                .input(&state, branch)
                .map_err(SearchError::Domain)?;
            if input.is_some()
                && max_total_engine_steps.saturating_sub(used) < max_steps_per_transition
            {
                return Err(SearchError::Domain("replay engine budget reached".into()));
            }
            let next = self
                .space
                .transition(
                    &state,
                    branch,
                    CombatStepLimits {
                        max_engine_steps: max_steps_per_transition,
                        deadline,
                    },
                )
                .map_err(SearchError::Domain)?;
            let Transition::Settled {
                state: successor,
                engine_steps,
            } = next
            else {
                return Err(SearchError::Domain("replay did not settle".into()));
            };
            used += engine_steps;
            if &successor != expected_after {
                return Err(SearchError::Domain("replay successor changed".into()));
            }
            if let Some(input) = input {
                inputs.push(input);
            }
            state = successor;
        }
        if !EngineCombatStepper.is_exact_victory(&state.position) {
            return Err(SearchError::Domain("replay did not end in victory".into()));
        }
        Ok(ReplayedCombatWin {
            root,
            inputs,
            successor: state.position().clone(),
            engine_steps: used,
        })
    }
}
