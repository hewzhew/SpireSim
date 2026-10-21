//! Batched in-process execution for online-learning environments.
//!
//! Ordinary steps apply one already-selected action per active slot; the pool
//! owns no tensor encoder, reset curriculum, or automatic recovery rule.
//! An explicit fixed-combat macro is opt-in and records its separate oracle policy.

#[path = "learning_fixed_combat.rs"]
mod fixed_combat;
pub use fixed_combat::*;

use std::collections::BTreeSet;
use std::fmt;

use super::{
    learning_public_information_snapshot_with_surface_v1, CombatLearningPotionPolicyV1,
    CombatLearningRootContextV1, LearningActionV1, LearningBoundaryV1, LearningCandidateSurfaceV1,
    LearningEnvCheckpointV3, LearningEnvV3, LearningModelBatchV1, LearningModelChoiceV1,
    LearningModelDecisionV1, LearningModelInputError, LearningPublicRunContextV1,
    LearningSelectionStepV1, LearningTerminalOutcomeV1, RunControlConfig,
    RunControlSessionCheckpointV1,
};
use crate::ai::planner_core::stable_planner_id;

#[derive(Clone, Eq, PartialEq)]
pub struct LearningDecisionTokenV3 {
    pub token_id: String,
    pub decision_step: u64,
    pub public_snapshot_id: String,
    lineage_id: String,
    exact_state_id: String,
}

impl fmt::Debug for LearningDecisionTokenV3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LearningDecisionTokenV3")
            .field("token_id", &self.token_id)
            .field("decision_step", &self.decision_step)
            .field("public_snapshot_id", &self.public_snapshot_id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningDecisionPathV3 {
    pub source: LearningDecisionTokenV3,
    pub ordinals: Vec<usize>,
}

/// External inputs for one real commit, aligned with active slots.
/// Execution metadata stays separate from the public policy observation.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningExecutionInputsV1 {
    pub secret_portal_playtime_bits: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LearningTransitionV3 {
    pub slot_index: usize,
    pub source: LearningDecisionTokenV3,
    pub action_ordinals: Vec<usize>,
    pub reward: i8,
    pub next: Option<LearningDecisionTokenV3>,
    pub terminal_outcome: Option<LearningTerminalOutcomeV1>,
}

#[derive(Clone, Debug)]
struct LearningEnvPoolSlotV3 {
    env: LearningEnvV3,
    boundary: LearningBoundaryV1,
}

#[derive(Clone, Debug)]
pub struct LearningEnvPoolV3 {
    slots: Vec<LearningEnvPoolSlotV3>,
    potion_policy: CombatLearningPotionPolicyV1,
    candidate_surface: LearningCandidateSurfaceV1,
    poisoned: bool,
}

impl LearningEnvPoolV3 {
    pub fn from_configs(
        configs: impl IntoIterator<Item = RunControlConfig>,
    ) -> Result<Self, LearningEnvPoolError> {
        Self::from_envs(configs.into_iter().map(LearningEnvV3::new))
    }

    pub fn from_envs(
        envs: impl IntoIterator<Item = LearningEnvV3>,
    ) -> Result<Self, LearningEnvPoolError> {
        Self::from_envs_with_potion_policy(envs, CombatLearningPotionPolicyV1::All)
    }

    pub fn from_envs_with_potion_policy(
        envs: impl IntoIterator<Item = LearningEnvV3>,
        potion_policy: CombatLearningPotionPolicyV1,
    ) -> Result<Self, LearningEnvPoolError> {
        let envs = envs.into_iter();
        let (lower_bound, _) = envs.size_hint();
        let mut slots = Vec::with_capacity(lower_bound);
        for (slot_index, mut env) in envs.enumerate() {
            let boundary = env.settle_boundary().map_err(|message| {
                LearningEnvPoolError::InitialObservation {
                    slot_index,
                    message,
                }
            })?;
            slots.push(LearningEnvPoolSlotV3 { env, boundary });
        }
        Ok(Self {
            slots,
            potion_policy,
            candidate_surface: LearningCandidateSurfaceV1::Policy,
            poisoned: false,
        })
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn active_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| !slot.boundary.is_terminal())
            .count()
    }

    pub fn terminal_count(&self) -> usize {
        self.slot_count().saturating_sub(self.active_count())
    }

    pub fn all_terminal(&self) -> bool {
        self.active_count() == 0
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    pub fn potion_policy(&self) -> &CombatLearningPotionPolicyV1 {
        &self.potion_policy
    }

    pub fn with_candidate_surface(mut self, surface: LearningCandidateSurfaceV1) -> Self {
        self.candidate_surface = surface;
        self
    }

    pub fn candidate_surface(&self) -> LearningCandidateSurfaceV1 {
        self.candidate_surface
    }

    /// Private diagnostic identities excluding controller-only counters.
    /// These state hashes must not be used as model features.
    pub fn continuation_fingerprints_v1(&self) -> Vec<String> {
        self.slots
            .iter()
            .map(|slot| {
                super::run_control_continuation_fingerprint_v1(&slot.env.clone().into_session())
            })
            .collect()
    }

    pub fn boundary(&self, slot_index: usize) -> Option<&LearningBoundaryV1> {
        self.slots.get(slot_index).map(|slot| &slot.boundary)
    }

    /// Captures one exact in-memory slot for a caller-owned recovery or
    /// curriculum policy. Nothing is serialized and the pool does not retain
    /// an automatic checkpoint history.
    pub fn checkpoint_slot(
        &self,
        slot_index: usize,
    ) -> Result<LearningEnvCheckpointV3, LearningEnvPoolError> {
        if self.poisoned {
            return Err(LearningEnvPoolError::PoolPoisoned);
        }
        let slot = self
            .slots
            .get(slot_index)
            .ok_or(LearningEnvPoolError::SlotIndexOutOfRange {
                slot_index,
                slot_count: self.slots.len(),
            })?;
        Ok(slot.env.checkpoint())
    }

    #[doc(hidden)]
    pub fn session_checkpoint_slot(
        &self,
        slot_index: usize,
    ) -> Result<RunControlSessionCheckpointV1, LearningEnvPoolError> {
        if self.poisoned {
            return Err(LearningEnvPoolError::PoolPoisoned);
        }
        let slot = self
            .slots
            .get(slot_index)
            .ok_or(LearningEnvPoolError::SlotIndexOutOfRange {
                slot_index,
                slot_count: self.slots.len(),
            })?;
        Ok(slot.env.session_checkpoint())
    }

    /// Return the exact durable run configuration for replacing one slot with
    /// a fresh seed.  This deliberately derives from the live slot instead of
    /// `RunControlConfig::default()` so resets cannot silently change the
    /// experiment's ascension or other environment settings.
    pub fn fresh_run_config(
        &self,
        slot_index: usize,
        seed: u64,
    ) -> Result<RunControlConfig, LearningEnvPoolError> {
        if self.poisoned {
            return Err(LearningEnvPoolError::PoolPoisoned);
        }
        let slot = self
            .slots
            .get(slot_index)
            .ok_or(LearningEnvPoolError::SlotIndexOutOfRange {
                slot_index,
                slot_count: self.slots.len(),
            })?;
        Ok(slot.env.fresh_run_config(seed))
    }

    /// Returns compact public metadata for one current combat root without cloning its session.
    pub fn combat_root_context(
        &self,
        slot_index: usize,
    ) -> Result<CombatLearningRootContextV1, LearningEnvPoolError> {
        if self.poisoned {
            return Err(LearningEnvPoolError::PoolPoisoned);
        }
        let slot = self
            .slots
            .get(slot_index)
            .ok_or(LearningEnvPoolError::SlotIndexOutOfRange {
                slot_index,
                slot_count: self.slots.len(),
            })?;
        slot.env
            .combat_root_context()
            .map_err(|message| LearningEnvPoolError::CombatRootContext {
                slot_index,
                message,
            })
    }

    /// Returns compact public run facts for one current slot without cloning its session.
    pub fn public_run_context(
        &self,
        slot_index: usize,
    ) -> Result<LearningPublicRunContextV1, LearningEnvPoolError> {
        if self.poisoned {
            return Err(LearningEnvPoolError::PoolPoisoned);
        }
        let slot = self
            .slots
            .get(slot_index)
            .ok_or(LearningEnvPoolError::SlotIndexOutOfRange {
                slot_index,
                slot_count: self.slots.len(),
            })?;
        slot.env
            .public_run_context(&slot.boundary)
            .map_err(|message| LearningEnvPoolError::PublicRunContext {
                slot_index,
                message,
            })
    }

    pub fn decision_token(
        &self,
        slot_index: usize,
    ) -> Result<LearningDecisionTokenV3, LearningEnvPoolError> {
        let slot = self
            .slots
            .get(slot_index)
            .ok_or(LearningEnvPoolError::SlotIndexOutOfRange {
                slot_index,
                slot_count: self.slots.len(),
            })?;
        if slot.boundary.is_terminal() {
            return Err(LearningEnvPoolError::TerminalDecisionToken { slot_index });
        }
        let snapshot = learning_public_information_snapshot_with_surface_v1(
            &slot.boundary,
            &self.potion_policy,
            self.candidate_surface,
        )
        .map_err(|message| LearningEnvPoolError::DecisionIdentity {
            slot_index,
            message,
        })?;
        let decision_step = slot.env.decision_step();
        let exact_state_id = slot.env.exact_state_id();
        let lineage_id = slot.env.lineage_id().to_owned();
        let token_id = stable_planner_id(
            "learning_decision_token_v3",
            &(
                lineage_id.as_str(),
                decision_step,
                exact_state_id.as_str(),
                snapshot.snapshot_id.as_str(),
                self.candidate_surface,
            ),
        )
        .map_err(|message| LearningEnvPoolError::DecisionIdentity {
            slot_index,
            message,
        })?;
        Ok(LearningDecisionTokenV3 {
            token_id,
            decision_step,
            public_snapshot_id: snapshot.snapshot_id,
            lineage_id,
            exact_state_id,
        })
    }

    /// Explicitly replaces one slot after a caller has chosen its reset or
    /// curriculum policy. The replacement is observed before the old slot is
    /// changed, so an invalid environment leaves the pool untouched.
    pub fn replace_slot(
        &mut self,
        slot_index: usize,
        env: LearningEnvV3,
    ) -> Result<(), LearningEnvPoolError> {
        self.replace_slots([(slot_index, env)])
    }

    /// Atomically replaces an arbitrary set of slots.
    ///
    /// Every target and replacement observation is validated before the first
    /// slot changes, so a malformed recovery batch cannot partially restore a
    /// pool.
    pub fn replace_slots(
        &mut self,
        replacements: impl IntoIterator<Item = (usize, LearningEnvV3)>,
    ) -> Result<(), LearningEnvPoolError> {
        if self.poisoned {
            return Err(LearningEnvPoolError::PoolPoisoned);
        }
        let mut seen = BTreeSet::new();
        let mut prepared = Vec::new();
        for (slot_index, mut env) in replacements {
            if slot_index >= self.slots.len() {
                return Err(LearningEnvPoolError::SlotIndexOutOfRange {
                    slot_index,
                    slot_count: self.slots.len(),
                });
            }
            if !seen.insert(slot_index) {
                return Err(LearningEnvPoolError::DuplicateSlotIndex { slot_index });
            }
            let boundary = env.settle_boundary().map_err(|message| {
                LearningEnvPoolError::ReplacementObservation {
                    slot_index,
                    message,
                }
            })?;
            prepared.push((slot_index, LearningEnvPoolSlotV3 { env, boundary }));
        }
        for (slot_index, slot) in prepared {
            self.slots[slot_index] = slot;
        }
        Ok(())
    }

    pub fn reset_slot(
        &mut self,
        slot_index: usize,
        config: RunControlConfig,
    ) -> Result<(), LearningEnvPoolError> {
        self.reset_slots([(slot_index, config)])
    }

    pub fn reset_slots(
        &mut self,
        resets: impl IntoIterator<Item = (usize, RunControlConfig)>,
    ) -> Result<(), LearningEnvPoolError> {
        self.replace_slots(
            resets
                .into_iter()
                .map(|(slot_index, config)| (slot_index, LearningEnvV3::new(config))),
        )
    }

    pub fn active_model_batch(
        &self,
    ) -> Result<LearningEnvPoolModelBatchV3<'_>, LearningEnvPoolError> {
        if self.poisoned {
            return Err(LearningEnvPoolError::PoolPoisoned);
        }
        let active_slot_indices = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot_index, slot)| (!slot.boundary.is_terminal()).then_some(slot_index))
            .collect::<Vec<_>>();
        let decisions = active_slot_indices
            .iter()
            .map(|slot_index| {
                LearningModelDecisionV1::from_boundary_with_surface(
                    &self.slots[*slot_index].boundary,
                    &self.potion_policy,
                    self.candidate_surface,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(LearningEnvPoolError::ModelInput)?;
        let model_batch = LearningModelBatchV1::from_decisions(decisions)
            .map_err(LearningEnvPoolError::ModelInput)?;
        let decision_tokens = active_slot_indices
            .iter()
            .map(|slot_index| self.decision_token(*slot_index))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LearningEnvPoolModelBatchV3 {
            active_slot_indices,
            decision_tokens,
            model_batch,
        })
    }

    /// Executes model-visible ordinal paths and returns one lineage-complete
    /// transition per active slot. Exact actions never cross this boundary.
    pub fn step_active_paths_v3(
        &mut self,
        paths: Vec<LearningDecisionPathV3>,
    ) -> Result<Vec<LearningTransitionV3>, LearningEnvPoolError> {
        self.step_active_paths_inner_v3(paths, None)
    }

    /// Re-execute recorded environmental inputs atomically. Rejected tapes
    /// leave the whole source batch and its decision tokens unchanged.
    pub fn step_active_paths_with_inputs_v1(
        &mut self,
        paths: Vec<LearningDecisionPathV3>,
        inputs: Vec<LearningExecutionInputsV1>,
    ) -> Result<Vec<LearningTransitionV3>, LearningEnvPoolError> {
        if inputs.len() != paths.len() {
            return Err(LearningEnvPoolError::ExecutionInputCountMismatch {
                path_count: paths.len(),
                input_count: inputs.len(),
            });
        }
        let mut trial = self.clone();
        let transitions = trial.step_active_paths_inner_v3(paths, Some(inputs))?;
        *self = trial;
        Ok(transitions)
    }

    fn step_active_paths_inner_v3(
        &mut self,
        paths: Vec<LearningDecisionPathV3>,
        execution_inputs: Option<Vec<LearningExecutionInputsV1>>,
    ) -> Result<Vec<LearningTransitionV3>, LearningEnvPoolError> {
        if self.poisoned {
            return Err(LearningEnvPoolError::PoolPoisoned);
        }
        let active_slot_indices = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot_index, slot)| (!slot.boundary.is_terminal()).then_some(slot_index))
            .collect::<Vec<_>>();
        if paths.len() != active_slot_indices.len() {
            return Err(LearningEnvPoolError::ActionCountMismatch {
                active_slot_count: active_slot_indices.len(),
                action_count: paths.len(),
            });
        }

        let mut prepared = Vec::with_capacity(paths.len());
        for (slot_index, path) in active_slot_indices.iter().copied().zip(paths) {
            if path.source != self.decision_token(slot_index)? {
                return Err(LearningEnvPoolError::StaleDecisionToken { slot_index });
            }
            let decision = LearningModelDecisionV1::from_boundary_with_surface(
                &self.slots[slot_index].boundary,
                &self.potion_policy,
                self.candidate_surface,
            )
            .map_err(LearningEnvPoolError::ModelInput)?;
            let action = resolve_decision_path_v3(&decision, &path.ordinals)
                .map_err(LearningEnvPoolError::ModelInput)?;
            let prepared_action =
                self.slots[slot_index]
                    .env
                    .prepare_action(action)
                    .map_err(|message| LearningEnvPoolError::InvalidAction {
                        slot_index,
                        message,
                    })?;
            prepared.push((path, prepared_action));
        }

        let mut transitions = Vec::with_capacity(prepared.len());
        for (row, (slot_index, (path, action))) in
            active_slot_indices.into_iter().zip(prepared).enumerate()
        {
            let scope = execution_inputs
                .as_ref()
                .map(|inputs| {
                    crate::runtime::event_time::EventTimeScope::begin(
                        inputs[row]
                            .secret_portal_playtime_bits
                            .iter()
                            .copied()
                            .map(f32::from_bits)
                            .collect(),
                    )
                    .map_err(|message| LearningEnvPoolError::ExecutionInputs {
                        slot_index,
                        message,
                    })
                })
                .transpose()?;
            let result = self.slots[slot_index].env.step_prepared(action);
            if let Some(scope) = scope {
                let (provided, consumed) = scope.finish();
                if provided != consumed {
                    return Err(LearningEnvPoolError::ExecutionInputs {
                        slot_index,
                        message: format!(
                            "event time tape supplied {provided} values but consumed {consumed}"
                        ),
                    });
                }
            }
            let step = match result {
                Ok(step) => step,
                Err(message) => {
                    self.poisoned = true;
                    return Err(LearningEnvPoolError::EngineStep {
                        slot_index,
                        message,
                    });
                }
            };
            let terminal_outcome = match &step.boundary {
                LearningBoundaryV1::Terminal { outcome } => Some(outcome.clone()),
                _ => None,
            };
            self.slots[slot_index].boundary = step.boundary;
            let next = if terminal_outcome.is_none() {
                match self.decision_token(slot_index) {
                    Ok(token) => Some(token),
                    Err(error) => {
                        self.poisoned = true;
                        return Err(error);
                    }
                }
            } else {
                None
            };
            transitions.push(LearningTransitionV3 {
                slot_index,
                source: path.source,
                action_ordinals: path.ordinals,
                reward: step.reward,
                next,
                terminal_outcome,
            });
        }
        Ok(transitions)
    }
}

fn resolve_decision_path_v3(
    decision: &LearningModelDecisionV1<'_>,
    ordinals: &[usize],
) -> Result<LearningActionV1, LearningModelInputError> {
    let Some((&root, suffix)) = ordinals.split_first() else {
        return Err(LearningModelInputError::EmptyDecisionPath);
    };
    match decision.choose(root)? {
        LearningModelChoiceV1::Apply(action) => {
            if suffix.is_empty() {
                Ok(action)
            } else {
                Err(LearningModelInputError::DecisionPathContinuesAfterApply)
            }
        }
        LearningModelChoiceV1::DecodeSelection(mut draft) => {
            if suffix.is_empty() {
                return Err(LearningModelInputError::DecisionPathEndsBeforeApply);
            }
            for (index, ordinal) in suffix.iter().copied().enumerate() {
                match draft.choose(ordinal)? {
                    LearningSelectionStepV1::Continue if index + 1 < suffix.len() => {}
                    LearningSelectionStepV1::Continue => {
                        return Err(LearningModelInputError::DecisionPathEndsBeforeApply)
                    }
                    LearningSelectionStepV1::Apply(action) if index + 1 == suffix.len() => {
                        return Ok(action)
                    }
                    LearningSelectionStepV1::Apply(_) => {
                        return Err(LearningModelInputError::DecisionPathContinuesAfterApply)
                    }
                }
            }
            Err(LearningModelInputError::DecisionPathEndsBeforeApply)
        }
    }
}

#[derive(Clone, Debug)]
pub struct LearningEnvPoolModelBatchV3<'a> {
    /// Row `n` in `model_batch` belongs to this environment slot.
    pub active_slot_indices: Vec<usize>,
    pub decision_tokens: Vec<LearningDecisionTokenV3>,
    pub model_batch: LearningModelBatchV1<'a>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LearningEnvPoolError {
    ExecutionInputCountMismatch {
        path_count: usize,
        input_count: usize,
    },
    ExecutionInputs {
        slot_index: usize,
        message: String,
    },
    InitialObservation {
        slot_index: usize,
        message: String,
    },
    ReplacementObservation {
        slot_index: usize,
        message: String,
    },
    CombatRootContext {
        slot_index: usize,
        message: String,
    },
    PublicRunContext {
        slot_index: usize,
        message: String,
    },
    ModelInput(LearningModelInputError),
    SlotIndexOutOfRange {
        slot_index: usize,
        slot_count: usize,
    },
    DuplicateSlotIndex {
        slot_index: usize,
    },
    ActionCountMismatch {
        active_slot_count: usize,
        action_count: usize,
    },
    InvalidAction {
        slot_index: usize,
        message: String,
    },
    EngineStep {
        slot_index: usize,
        message: String,
    },
    StaleDecisionToken {
        slot_index: usize,
    },
    TerminalDecisionToken {
        slot_index: usize,
    },
    DecisionIdentity {
        slot_index: usize,
        message: String,
    },
    PoolPoisoned,
}

impl fmt::Display for LearningEnvPoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for LearningEnvPoolError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::potions::{Potion, PotionId};
    use crate::eval::run_control::{
        LearningBoundaryKindV1, LearningStrategicContextKindV1, RunControlSession,
    };
    use crate::state::core::RunResult;

    #[test]
    fn candidate_surface_is_bound_to_tokens_and_rejects_cross_surface_paths() {
        let policy = LearningEnvPoolV3::from_configs([RunControlConfig::default()]).unwrap();
        let mut legal = policy
            .clone()
            .with_candidate_surface(LearningCandidateSurfaceV1::Legal);
        let before = legal.checkpoint_slot(0).unwrap();
        assert_ne!(
            policy.decision_token(0).unwrap(),
            legal.decision_token(0).unwrap()
        );
        assert_eq!(
            legal
                .step_active_paths_v3(vec![LearningDecisionPathV3 {
                    source: policy.decision_token(0).unwrap(),
                    ordinals: vec![0],
                }])
                .unwrap_err(),
            LearningEnvPoolError::StaleDecisionToken { slot_index: 0 }
        );
        assert_eq!(before, legal.checkpoint_slot(0).unwrap());
        let source = legal.decision_token(0).unwrap();
        let transitions = legal
            .step_active_paths_v3(vec![LearningDecisionPathV3 {
                source: source.clone(),
                ordinals: vec![0],
            }])
            .unwrap();
        assert_eq!(transitions[0].source, source);
        assert_eq!(legal.candidate_surface(), LearningCandidateSurfaceV1::Legal);
    }

    #[test]
    fn recorded_inputs_reject_a_bad_second_slot_without_advancing_the_first() {
        let mut pool =
            LearningEnvPoolV3::from_configs(vec![RunControlConfig::default(); 2]).unwrap();
        let before = (0..2)
            .map(|i| pool.checkpoint_slot(i).unwrap())
            .collect::<Vec<_>>();
        let paths = (0..2)
            .map(|i| LearningDecisionPathV3 {
                source: pool.decision_token(i).unwrap(),
                ordinals: vec![0],
            })
            .collect::<Vec<_>>();
        for value in [17.0f32.to_bits(), f32::NAN.to_bits(), (-0.0f32).to_bits()] {
            let error = pool
                .step_active_paths_with_inputs_v1(
                    paths.clone(),
                    vec![
                        LearningExecutionInputsV1::default(),
                        LearningExecutionInputsV1 {
                            secret_portal_playtime_bits: vec![value],
                        },
                    ],
                )
                .unwrap_err();
            assert!(matches!(
                error,
                LearningEnvPoolError::ExecutionInputs { slot_index: 1, .. }
            ));
            assert!(!pool.is_poisoned());
            assert_eq!(
                before,
                (0..2)
                    .map(|i| pool.checkpoint_slot(i).unwrap())
                    .collect::<Vec<_>>()
            );
        }
        let mut standalone = pool.clone();
        let expected = standalone.step_active_paths_v3(paths.clone()).unwrap();
        let actual = pool
            .step_active_paths_with_inputs_v1(paths, vec![LearningExecutionInputsV1::default(); 2])
            .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            pool.continuation_fingerprints_v1(),
            standalone.continuation_fingerprints_v1()
        );
    }

    #[test]
    fn pool_settles_empty_campfire_on_admission_and_replacement() {
        use crate::content::relics::{RelicId, RelicState};
        use crate::state::core::EngineState;
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::Campfire;
        session.run_state.is_final_act_available = false;
        session.run_state.relics = [RelicId::CoffeeDripper, RelicId::FusionHammer]
            .into_iter()
            .map(RelicState::new)
            .collect();
        session.run_state.potions = vec![Some(Potion::new(PotionId::SwiftPotion, 10))];
        let source = LearningEnvV3::from_session(session);
        let mut pool = LearningEnvPoolV3::from_envs([source.clone()]).unwrap();
        assert_eq!(
            pool.boundary(0).unwrap().strategic_context_kind(),
            Some(LearningStrategicContextKindV1::Map)
        );
        let token = pool.decision_token(0).unwrap();
        assert_eq!(token.decision_step, 0);
        assert_eq!(
            pool.public_run_context(0).unwrap().potion_ids,
            vec![Some(PotionId::SwiftPotion)]
        );
        pool.replace_slots([(0, source)]).unwrap();
        assert_eq!(
            pool.boundary(0).unwrap().strategic_context_kind(),
            Some(LearningStrategicContextKindV1::Map)
        );
        assert_eq!(pool.decision_token(0).unwrap().decision_step, 0);
    }

    #[test]
    fn public_run_context_exposes_typed_resources_without_a_checkpoint_clone() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 2;
        session.run_state.floor_num = 19;
        session.run_state.current_hp = 41;
        session.run_state.max_hp = 85;
        session.run_state.gold = 123;
        session.run_state.potions = vec![Some(Potion::new(PotionId::GamblersBrew, 0)), None];
        let seed = session.run_state.seed;
        let pool = LearningEnvPoolV3::from_envs([LearningEnvV3::from_session(session)])
            .expect("create pool");

        let context = pool.public_run_context(0).expect("public run context");

        assert_eq!(context.boundary_kind, LearningBoundaryKindV1::Strategic);
        assert_eq!(
            context.strategic_context_kind,
            Some(LearningStrategicContextKindV1::Event)
        );
        assert_eq!(context.seed, seed);
        assert_eq!(context.ascension_level, 0);
        assert_eq!((context.act, context.floor), (2, 19));
        assert_eq!((context.hp, context.max_hp, context.gold), (41, 85, 123));
        assert_eq!(context.potion_ids, vec![Some(PotionId::GamblersBrew), None]);
    }

    #[test]
    fn empty_pool_is_a_valid_empty_batch() {
        let pool = LearningEnvPoolV3::from_configs([]).expect("create empty pool");
        let batch = pool.active_model_batch().expect("build empty model batch");

        assert_eq!(pool.slot_count(), 0);
        assert!(pool.all_terminal());
        assert!(batch.active_slot_indices.is_empty());
        assert!(batch.model_batch.decisions.is_empty());
        assert_eq!(batch.model_batch.candidate_row_splits, vec![0]);
    }

    #[test]
    fn slot_checkpoint_is_explicit_exact_and_bounds_checked() {
        let pool = LearningEnvPoolV3::from_configs([RunControlConfig {
            seed: 17,
            ..RunControlConfig::default()
        }])
        .expect("create pool");

        assert_eq!(
            pool.checkpoint_slot(0).expect("checkpoint first slot"),
            pool.slots[0].env.checkpoint()
        );
        assert_eq!(
            pool.checkpoint_slot(1)
                .expect_err("missing slot must not produce a checkpoint"),
            LearningEnvPoolError::SlotIndexOutOfRange {
                slot_index: 1,
                slot_count: 1,
            }
        );
    }

    #[test]
    fn checkpoint_restore_preserves_token_while_fresh_equal_state_gets_new_lineage() {
        let env = LearningEnvV3::new(RunControlConfig {
            seed: 17,
            ..RunControlConfig::default()
        });
        let checkpoint = env.checkpoint();
        let original = LearningEnvPoolV3::from_envs([env])
            .expect("create original pool")
            .decision_token(0)
            .expect("original token");
        let restored = LearningEnvPoolV3::from_envs([
            LearningEnvV3::from_checkpoint(checkpoint).expect("restore environment")
        ])
        .expect("create restored pool")
        .decision_token(0)
        .expect("restored token");
        let fresh = LearningEnvPoolV3::from_configs([RunControlConfig {
            seed: 17,
            ..RunControlConfig::default()
        }])
        .expect("create fresh equal-state pool")
        .decision_token(0)
        .expect("fresh token");

        assert_eq!(restored, original);
        assert_eq!(fresh.public_snapshot_id, original.public_snapshot_id);
        assert_ne!(fresh.token_id, original.token_id);
    }

    #[test]
    fn fresh_run_config_preserves_environment_settings_and_changes_only_seed() {
        let pool = LearningEnvPoolV3::from_configs([RunControlConfig {
            seed: 17,
            ascension_level: 20,
            final_act: true,
            search_max_nodes: Some(1234),
            search_wall_ms: Some(5678),
            search_max_hp_loss: Some(9),
            search_max_potions_used: Some(1),
            ..RunControlConfig::default()
        }])
        .expect("create pool");

        let replacement = pool
            .fresh_run_config(0, 99)
            .expect("derive replacement config");

        assert_eq!(replacement.seed, 99);
        assert_eq!(replacement.ascension_level, 20);
        assert!(replacement.final_act);
        assert_eq!(replacement.player_class, "Ironclad");
        assert_eq!(replacement.search_max_nodes, Some(1234));
        assert_eq!(replacement.search_wall_ms, Some(5678));
        assert_eq!(replacement.search_max_hp_loss, Some(9));
        assert_eq!(replacement.search_max_potions_used, Some(1));
    }

    #[test]
    fn replacement_batch_rejects_duplicate_or_missing_targets_without_partial_mutation() {
        // Both rejection paths must be checked against the same retained pool
        // snapshots to prove that validation never commits the valid prefix.
        let mut pool = LearningEnvPoolV3::from_configs([
            RunControlConfig {
                seed: 17,
                ..RunControlConfig::default()
            },
            RunControlConfig {
                seed: 18,
                ..RunControlConfig::default()
            },
        ])
        .expect("create pool");
        let before = (0..pool.slot_count())
            .map(|slot| pool.checkpoint_slot(slot).expect("checkpoint slot"))
            .collect::<Vec<_>>();

        assert_eq!(
            pool.replace_slots([
                (
                    0,
                    LearningEnvV3::new(RunControlConfig {
                        seed: 70,
                        ..RunControlConfig::default()
                    }),
                ),
                (
                    0,
                    LearningEnvV3::new(RunControlConfig {
                        seed: 71,
                        ..RunControlConfig::default()
                    }),
                ),
            ])
            .expect_err("duplicate targets must fail"),
            LearningEnvPoolError::DuplicateSlotIndex { slot_index: 0 }
        );
        assert_eq!(
            pool.replace_slots([
                (
                    0,
                    LearningEnvV3::new(RunControlConfig {
                        seed: 72,
                        ..RunControlConfig::default()
                    }),
                ),
                (
                    2,
                    LearningEnvV3::new(RunControlConfig {
                        seed: 73,
                        ..RunControlConfig::default()
                    }),
                ),
            ])
            .expect_err("missing target must fail"),
            LearningEnvPoolError::SlotIndexOutOfRange {
                slot_index: 2,
                slot_count: 2,
            }
        );
        assert_eq!(
            (0..pool.slot_count())
                .map(|slot| pool.checkpoint_slot(slot).expect("checkpoint slot"))
                .collect::<Vec<_>>(),
            before
        );
    }

    #[test]
    fn stale_token_is_rejected_before_any_slot_advances() {
        let mut pool = LearningEnvPoolV3::from_configs([
            RunControlConfig {
                seed: 1,
                ..RunControlConfig::default()
            },
            RunControlConfig {
                seed: 2,
                ..RunControlConfig::default()
            },
        ])
        .expect("create pool");
        let before = pool
            .slots
            .iter()
            .map(|slot| slot.env.checkpoint())
            .collect::<Vec<_>>();
        let first = pool.decision_token(0).expect("first token");
        let second = pool.decision_token(1).expect("second token");
        let mut stale_second = second.clone();
        stale_second.token_id.push_str("-stale");

        assert_eq!(
            pool.step_active_paths_v3(vec![
                LearningDecisionPathV3 {
                    source: first,
                    ordinals: vec![0],
                },
                LearningDecisionPathV3 {
                    source: stale_second,
                    ordinals: vec![0],
                },
            ])
            .expect_err("stale token must fail"),
            LearningEnvPoolError::StaleDecisionToken { slot_index: 1 }
        );
        assert!(!pool.is_poisoned());
        assert_eq!(
            pool.slots
                .iter()
                .map(|slot| slot.env.checkpoint())
                .collect::<Vec<_>>(),
            before
        );
    }

    #[test]
    fn terminal_slots_drop_out_of_the_next_model_batch() {
        let mut terminal_session = RunControlSession::new(RunControlConfig::default());
        terminal_session.engine_state =
            crate::state::core::EngineState::GameOver(RunResult::Defeat);
        let terminal = LearningEnvV3::from_session(terminal_session);
        let active = LearningEnvV3::new(RunControlConfig {
            seed: 2,
            ..RunControlConfig::default()
        });
        let pool = LearningEnvPoolV3::from_envs([terminal, active]).expect("create mixed pool");
        let batch = pool.active_model_batch().expect("build active batch");

        assert_eq!(pool.slot_count(), 2);
        assert_eq!(pool.terminal_count(), 1);
        assert_eq!(batch.active_slot_indices, vec![1]);
        assert_eq!(batch.model_batch.decisions.len(), 1);
    }

    #[test]
    fn terminal_slot_returns_only_after_an_explicit_reset() {
        let mut terminal_session = RunControlSession::new(RunControlConfig::default());
        terminal_session.engine_state =
            crate::state::core::EngineState::GameOver(RunResult::Defeat);
        let terminal = LearningEnvV3::from_session(terminal_session);
        let mut pool = LearningEnvPoolV3::from_envs([terminal]).expect("create terminal pool");

        assert!(pool.all_terminal());
        pool.reset_slot(
            0,
            RunControlConfig {
                seed: 99,
                ..RunControlConfig::default()
            },
        )
        .expect("explicitly reset terminal slot");
        let batch = pool.active_model_batch().expect("build reset batch");

        assert_eq!(pool.active_count(), 1);
        assert_eq!(batch.active_slot_indices, vec![0]);
        assert_eq!(batch.model_batch.decisions.len(), 1);
    }
}
