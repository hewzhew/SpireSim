//! Fixed same-root combat episode batches for grouped learning experiments.
//!
//! The pool owns neither a policy nor a combat objective. It preserves exact root and
//! replicate lineage, exposes one ragged model batch, and atomically validates a complete
//! active action batch before mutating any replicate.

use std::fmt;

use super::{
    CombatLearningBoundaryV1, CombatLearningEnvCheckpointV1, CombatLearningEnvV1,
    CombatLearningPotionPolicyV1, CombatLearningResourceSnapshotV1, CombatLearningRootContextV1,
    CombatLearningRootIdentityV1, CombatLearningRootV1, CombatLearningTerminalOutcomeV1,
    LearningActionV1, LearningModelBatchV1, LearningModelInputError,
};

#[derive(Clone, Debug)]
struct CombatLearningEnvPoolSlotV1 {
    env: CombatLearningEnvV1,
    boundary: CombatLearningBoundaryV1,
}

#[derive(Clone, Debug)]
pub struct CombatLearningEnvPoolV1 {
    root: CombatLearningRootIdentityV1,
    root_context: CombatLearningRootContextV1,
    root_resources: CombatLearningResourceSnapshotV1,
    potion_policy: CombatLearningPotionPolicyV1,
    slots: Vec<CombatLearningEnvPoolSlotV1>,
    poisoned: bool,
}

impl CombatLearningEnvPoolV1 {
    pub fn from_root(
        root: &CombatLearningRootV1,
        replicate_count: usize,
    ) -> Result<Self, CombatLearningEnvPoolError> {
        Self::from_root_with_potion_policy(root, replicate_count, CombatLearningPotionPolicyV1::All)
    }

    pub fn from_root_with_potion_policy(
        root: &CombatLearningRootV1,
        replicate_count: usize,
        potion_policy: CombatLearningPotionPolicyV1,
    ) -> Result<Self, CombatLearningEnvPoolError> {
        if replicate_count == 0 {
            return Err(CombatLearningEnvPoolError::EmptyReplicateGroup);
        }
        let last_replicate = replicate_count - 1;
        u32::try_from(last_replicate)
            .map_err(|_| CombatLearningEnvPoolError::ReplicateCountOverflow { replicate_count })?;

        let mut slots = Vec::with_capacity(replicate_count);
        for slot_index in 0..replicate_count {
            let replicate_index = slot_index as u32;
            let env = root.spawn(replicate_index).map_err(|message| {
                CombatLearningEnvPoolError::InitialObservation {
                    replicate_index,
                    message,
                }
            })?;
            let boundary = env.observe().map_err(|message| {
                CombatLearningEnvPoolError::InitialObservation {
                    replicate_index,
                    message,
                }
            })?;
            slots.push(CombatLearningEnvPoolSlotV1 { env, boundary });
        }
        Ok(Self {
            root: root.identity().clone(),
            root_context: *root.context(),
            root_resources: root.resources().clone(),
            potion_policy,
            slots,
            poisoned: false,
        })
    }

    pub fn from_root_with_potion_slots(
        root: &CombatLearningRootV1,
        replicate_count: usize,
        potion_slots: Option<Vec<usize>>,
    ) -> Result<Self, CombatLearningEnvPoolError> {
        let potion_policy = match potion_slots {
            None => CombatLearningPotionPolicyV1::All,
            Some(slots) => CombatLearningPotionPolicyV1::from_root_slots(root, slots)
                .map_err(CombatLearningEnvPoolError::PotionPolicy)?,
        };
        Self::from_root_with_potion_policy(root, replicate_count, potion_policy)
    }

    pub fn root_identity(&self) -> &CombatLearningRootIdentityV1 {
        &self.root
    }

    pub fn root_context(&self) -> &CombatLearningRootContextV1 {
        &self.root_context
    }

    pub fn root_resources(&self) -> &CombatLearningResourceSnapshotV1 {
        &self.root_resources
    }

    pub fn potion_policy(&self) -> &CombatLearningPotionPolicyV1 {
        &self.potion_policy
    }

    pub fn replicate_count(&self) -> usize {
        self.slots.len()
    }

    pub fn active_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| !slot.boundary.is_terminal())
            .count()
    }

    pub fn terminal_count(&self) -> usize {
        self.replicate_count().saturating_sub(self.active_count())
    }

    pub fn all_terminal(&self) -> bool {
        self.active_count() == 0
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    pub fn boundary(&self, replicate_index: u32) -> Option<&CombatLearningBoundaryV1> {
        self.slots
            .get(replicate_index as usize)
            .map(|slot| &slot.boundary)
    }

    pub fn checkpoint_replicate(
        &self,
        replicate_index: u32,
    ) -> Result<CombatLearningEnvCheckpointV1, CombatLearningEnvPoolError> {
        if self.poisoned {
            return Err(CombatLearningEnvPoolError::PoolPoisoned);
        }
        let slot = self.slots.get(replicate_index as usize).ok_or(
            CombatLearningEnvPoolError::ReplicateIndexOutOfRange {
                replicate_index,
                replicate_count: self.replicate_count(),
            },
        )?;
        Ok(slot.env.checkpoint())
    }

    /// Capture one active replicate's current exact state as a new combat root.
    ///
    /// The returned root owns a fresh identity for the current state. The pool
    /// remains unchanged; caller-facing adapters must retain the source pool
    /// identity and replicate index as explicit recovery lineage.
    pub fn current_root(
        &self,
        replicate_index: u32,
    ) -> Result<CombatLearningRootV1, CombatLearningEnvPoolError> {
        if self.poisoned {
            return Err(CombatLearningEnvPoolError::PoolPoisoned);
        }
        let slot = self.slots.get(replicate_index as usize).ok_or(
            CombatLearningEnvPoolError::ReplicateIndexOutOfRange {
                replicate_index,
                replicate_count: self.replicate_count(),
            },
        )?;
        slot.env
            .current_root()
            .map_err(|message| CombatLearningEnvPoolError::CurrentRoot {
                replicate_index,
                message,
            })
    }

    pub fn active_model_batch(
        &self,
    ) -> Result<CombatLearningEnvPoolModelBatchV1<'_>, CombatLearningEnvPoolError> {
        if self.poisoned {
            return Err(CombatLearningEnvPoolError::PoolPoisoned);
        }
        let mut active_replicate_indices = Vec::with_capacity(self.active_count());
        let mut boundaries = Vec::with_capacity(self.active_count());
        for slot in &self.slots {
            if let CombatLearningBoundaryV1::Decision { episode, boundary } = &slot.boundary {
                active_replicate_indices.push(episode.replicate_index);
                boundaries.push(boundary);
            }
        }
        let model_batch = LearningModelBatchV1::from_combat_boundary_refs_with_potion_policy(
            boundaries,
            &self.potion_policy,
        )
        .map_err(CombatLearningEnvPoolError::ModelInput)?;
        Ok(CombatLearningEnvPoolModelBatchV1 {
            root: &self.root,
            active_replicate_indices,
            model_batch,
        })
    }

    /// Applies one action per currently active replicate.
    ///
    /// Every action is checked against its unchanged combat boundary before the first slot
    /// advances. An engine failure after preparation poisons the group because its replicates
    /// may no longer describe one aligned transition round.
    pub fn step_active(
        &mut self,
        actions: Vec<LearningActionV1>,
    ) -> Result<CombatLearningEnvPoolStepV1, CombatLearningEnvPoolError> {
        if self.poisoned {
            return Err(CombatLearningEnvPoolError::PoolPoisoned);
        }
        let active_slot_indices = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot_index, slot)| (!slot.boundary.is_terminal()).then_some(slot_index))
            .collect::<Vec<_>>();
        if actions.len() != active_slot_indices.len() {
            return Err(CombatLearningEnvPoolError::ActionCountMismatch {
                active_replicate_count: active_slot_indices.len(),
                action_count: actions.len(),
            });
        }

        let mut prepared = Vec::with_capacity(actions.len());
        for (slot_index, action) in active_slot_indices.iter().copied().zip(actions) {
            let replicate_index = slot_index as u32;
            let input = self.slots[slot_index]
                .env
                .prepare_action(action)
                .map_err(|message| CombatLearningEnvPoolError::InvalidAction {
                    replicate_index,
                    message,
                })?;
            prepared.push(input);
        }

        let mut slots = Vec::with_capacity(prepared.len());
        for (slot_index, input) in active_slot_indices.into_iter().zip(prepared) {
            let replicate_index = slot_index as u32;
            let step = match self.slots[slot_index].env.step_prepared(input) {
                Ok(step) => step,
                Err(message) => {
                    self.poisoned = true;
                    return Err(CombatLearningEnvPoolError::EngineStep {
                        replicate_index,
                        message,
                    });
                }
            };
            let terminal_outcome = match &step.boundary {
                CombatLearningBoundaryV1::Terminal { outcome } => Some(outcome.clone()),
                CombatLearningBoundaryV1::Decision { .. } => None,
            };
            slots.push(CombatLearningEnvPoolSlotStepV1 {
                replicate_index,
                terminated: step.terminated,
                terminal_outcome,
            });
            self.slots[slot_index].boundary = step.boundary;
        }
        Ok(CombatLearningEnvPoolStepV1 { slots })
    }
}

#[derive(Clone, Debug)]
pub struct CombatLearningEnvPoolModelBatchV1<'a> {
    pub root: &'a CombatLearningRootIdentityV1,
    /// Row `n` in `model_batch` belongs to this same-root replicate.
    pub active_replicate_indices: Vec<u32>,
    pub model_batch: LearningModelBatchV1<'a>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatLearningEnvPoolSlotStepV1 {
    pub replicate_index: u32,
    pub terminated: bool,
    pub terminal_outcome: Option<CombatLearningTerminalOutcomeV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatLearningEnvPoolStepV1 {
    pub slots: Vec<CombatLearningEnvPoolSlotStepV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CombatLearningEnvPoolError {
    EmptyReplicateGroup,
    PotionPolicy(String),
    ReplicateCountOverflow {
        replicate_count: usize,
    },
    InitialObservation {
        replicate_index: u32,
        message: String,
    },
    ModelInput(LearningModelInputError),
    ReplicateIndexOutOfRange {
        replicate_index: u32,
        replicate_count: usize,
    },
    ActionCountMismatch {
        active_replicate_count: usize,
        action_count: usize,
    },
    InvalidAction {
        replicate_index: u32,
        message: String,
    },
    CurrentRoot {
        replicate_index: u32,
        message: String,
    },
    EngineStep {
        replicate_index: u32,
        message: String,
    },
    PoolPoisoned,
}

impl fmt::Display for CombatLearningEnvPoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CombatLearningEnvPoolError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::exordium::jaw_worm::JawWorm;
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::eval::run_control::RunControlSession;
    use crate::runtime::combat::CombatCard;
    use crate::sim::combat::CombatTerminal;
    use crate::state::core::{
        ActiveCombat, ClientInput, CombatContext, EngineState, RoomCombatContext,
    };
    use crate::state::map::node::RoomType;

    #[test]
    fn pool_exposes_one_ragged_batch_with_explicit_same_root_lineage() {
        let root =
            CombatLearningRootV1::from_session(combat_root_session(20)).expect("construct root");
        let pool = CombatLearningEnvPoolV1::from_root(&root, 3).expect("construct pool");
        let batch = pool.active_model_batch().expect("build model batch");

        assert_eq!(pool.root_identity(), root.identity());
        assert_eq!(pool.root_context(), root.context());
        assert_eq!(batch.root, root.identity());
        assert_eq!(batch.active_replicate_indices, vec![0, 1, 2]);
        assert_eq!(batch.model_batch.decisions.len(), 3);
        assert_eq!(batch.model_batch.candidate_row_splits.len(), 4);
    }

    #[test]
    fn invalid_action_rejects_the_whole_round_before_any_replicate_advances() {
        let root =
            CombatLearningRootV1::from_session(combat_root_session(20)).expect("construct root");
        let mut pool = CombatLearningEnvPoolV1::from_root(&root, 2).expect("construct pool");
        let before = [
            pool.checkpoint_replicate(0).unwrap(),
            pool.checkpoint_replicate(1).unwrap(),
        ];

        let error = pool
            .step_active(vec![
                LearningActionV1::CombatInput {
                    input: ClientInput::EndTurn,
                },
                LearningActionV1::StrategicCandidate {
                    candidate_id: "not-combat".to_string(),
                },
            ])
            .expect_err("mixed action kinds must fail atomically");

        assert!(matches!(
            error,
            CombatLearningEnvPoolError::InvalidAction {
                replicate_index: 1,
                ..
            }
        ));
        assert!(!pool.is_poisoned());
        assert_eq!(pool.checkpoint_replicate(0).unwrap(), before[0]);
        assert_eq!(pool.checkpoint_replicate(1).unwrap(), before[1]);
    }

    #[test]
    fn terminal_rows_retain_replicate_identity_and_leave_the_next_batch() {
        let root = CombatLearningRootV1::from_session(combat_root_session(1))
            .expect("construct lethal root");
        let mut pool = CombatLearningEnvPoolV1::from_root(&root, 2).expect("construct pool");
        let lethal = || LearningActionV1::CombatInput {
            input: ClientInput::PlayCard {
                card_index: 0,
                target: Some(7),
            },
        };

        let step = pool
            .step_active(vec![lethal(), lethal()])
            .expect("apply lethal batch");

        assert!(pool.all_terminal());
        assert_eq!(step.slots.len(), 2);
        for (expected, slot) in step.slots.iter().enumerate() {
            assert_eq!(slot.replicate_index, expected as u32);
            assert!(slot.terminated);
            let outcome = slot
                .terminal_outcome
                .as_ref()
                .expect("terminal row must retain typed outcome");
            assert_eq!(outcome.episode.root, *root.identity());
            assert_eq!(outcome.episode.replicate_index, expected as u32);
            assert_eq!(outcome.combat.terminal, CombatTerminal::Win);
        }
        let batch = pool.active_model_batch().expect("build empty active batch");
        assert!(batch.active_replicate_indices.is_empty());
        assert!(batch.model_batch.decisions.is_empty());
        assert_eq!(batch.model_batch.candidate_row_splits, vec![0]);
    }

    #[test]
    fn current_replicate_state_becomes_a_new_same_state_root() {
        let root = CombatLearningRootV1::from_session(combat_root_session(20))
            .expect("construct source root");
        let mut pool = CombatLearningEnvPoolV1::from_root(&root, 1).expect("construct pool");
        pool.step_active(vec![LearningActionV1::CombatInput {
            input: ClientInput::PlayCard {
                card_index: 0,
                target: Some(7),
            },
        }])
        .expect("advance source episode");

        let recovered = pool.current_root(0).expect("capture current root");
        assert_ne!(recovered.identity(), root.identity());
        let recovered_pool =
            CombatLearningEnvPoolV1::from_root(&recovered, 2).expect("spawn recovered group");
        assert_eq!(recovered_pool.root_identity(), recovered.identity());
        assert_eq!(recovered_pool.active_count(), 2);
        let CombatLearningBoundaryV1::Decision {
            boundary: first, ..
        } = recovered_pool.boundary(0).unwrap()
        else {
            panic!("recovered slot zero must be active");
        };
        let CombatLearningBoundaryV1::Decision {
            boundary: second, ..
        } = recovered_pool.boundary(1).unwrap()
        else {
            panic!("recovered slot one must be active");
        };
        assert_eq!(first, second);
    }

    #[test]
    fn empty_group_is_rejected() {
        let root =
            CombatLearningRootV1::from_session(combat_root_session(20)).expect("construct root");
        assert_eq!(
            CombatLearningEnvPoolV1::from_root(&root, 0)
                .expect_err("empty group has no estimator lineage"),
            CombatLearningEnvPoolError::EmptyReplicateGroup
        );
    }

    fn combat_root_session(monster_hp: i32) -> RunControlSession {
        let mut session = RunControlSession::new(Default::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 51)];
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.current_hp = monster_hp;
        monster.max_hp = monster_hp;
        monster.set_planned_move_id(1);
        let plan = JawWorm::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        session
    }
}
