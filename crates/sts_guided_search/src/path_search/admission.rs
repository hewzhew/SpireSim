//! Shared source-bound choice admission for runtime and route reconstruction.
use crate::combat::PreparedCombatChoices;
use crate::{CombatBranch, CombatSearchSpace, CombatSearchState, ResourceRequirement, Transition};
use sts_combat_outcome::combat_witness_materialization::starting_potion_slots_targeted_by_input_v1;
use sts_core::sim::combat::{CombatPosition, CombatStepLimits};
use sts_core::state::core::ClientInput;

pub(crate) struct AdmittedChoices {
    prepared: PreparedCombatChoices,
    indices: Vec<usize>,
}

impl AdmittedChoices {
    pub fn new(
        root: &CombatPosition,
        state: &CombatSearchState,
        goal: &ResourceRequirement,
    ) -> Result<Self, String> {
        let prepared = CombatSearchSpace::default().prepare_choices(state)?;
        let mut indices = Vec::new();
        for index in 0..prepared.branches().len() {
            if let Some(input) = prepared.input(index)? {
                if starting_potion_slots_targeted_by_input_v1(root, state.position(), &input)
                    & goal.preserve_starting_potion_slots
                    != 0
                {
                    continue;
                }
            }
            indices.push(index);
        }
        Ok(Self { prepared, indices })
    }

    pub fn len(&self) -> usize {
        self.indices.len()
    }
    pub fn rejected(&self) -> usize {
        self.prepared.branches().len() - self.len()
    }
    pub fn branch(&self, index: usize) -> &CombatBranch {
        &self.prepared.branches()[self.indices[index]]
    }
    pub fn branches(&self) -> Vec<CombatBranch> {
        self.indices
            .iter()
            .map(|i| self.prepared.branches()[*i].clone())
            .collect()
    }
    pub fn input(&self, index: usize) -> Result<Option<ClientInput>, String> {
        self.prepared.input(
            *self
                .indices
                .get(index)
                .ok_or("invalid admitted choice index")?,
        )
    }
    pub fn transition(
        &self,
        index: usize,
        limits: CombatStepLimits,
    ) -> Result<Transition<CombatSearchState>, String> {
        self.prepared.transition(
            *self
                .indices
                .get(index)
                .ok_or("invalid admitted choice index")?,
            limits,
        )
    }
    pub fn input_path(&self, input: &ClientInput) -> Result<Vec<CombatBranch>, String> {
        self.prepared.input_path(input)
    }
}
