//! One serializable local-graph planner contract shared by production and labs.

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::{
    root_initial_expansion_work_for_budget, CombatGuideLaneId, CombatGuideServiceBias,
    LocalTurnGraphWitnessConfig, LocalTurnGraphWitnessQuantum, OracleCombatWitnessSatisfaction,
    TurnOptionGeneratorConfig,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LocalTurnGraphGuideServiceBiasSpecV1 {
    lane: u32,
    extra_services_per_cycle: usize,
}

impl LocalTurnGraphGuideServiceBiasSpecV1 {
    pub fn new(lane: u32, extra_services_per_cycle: usize) -> Self {
        Self {
            lane,
            extra_services_per_cycle,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LocalTurnGraphPlannerSettingsV1 {
    max_engine_steps_per_transition: usize,
    uniform_exploration_ppm: u32,
    allow_potion_expenditure: bool,
    allow_potion_discard: bool,
    generation_quantum_work: usize,
    backed_generation_quantum_work: usize,
    guide_service_bias: Option<LocalTurnGraphGuideServiceBiasSpecV1>,
    initial_expansion_work: usize,
    root_initial_expansion_work: usize,
    max_turn_depth: usize,
    max_potions_used: Option<u32>,
    allowed_potion_slots: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LocalTurnGraphSearchAllowanceV1 {
    max_selections: usize,
    max_generation_work: usize,
    max_engine_steps: usize,
    wall_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LocalTurnGraphSearchSpecV1 {
    planner: LocalTurnGraphPlannerSettingsV1,
    allowance: LocalTurnGraphSearchAllowanceV1,
}

impl LocalTurnGraphSearchSpecV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn from_controls(
        max_generation_work: usize,
        max_selections: usize,
        wall_ms: u64,
        max_engine_steps_per_transition: usize,
        uniform_exploration_ppm: u32,
        generation_quantum_work: usize,
        max_turn_depth: usize,
        max_potions_used: Option<u32>,
        allow_potion_discard: bool,
        allowed_potion_slots: Option<u64>,
        initial_expansion_work: Option<usize>,
        guide_service_bias: Option<LocalTurnGraphGuideServiceBiasSpecV1>,
    ) -> Self {
        let defaults = LocalTurnGraphWitnessConfig::default();
        Self {
            planner: LocalTurnGraphPlannerSettingsV1 {
                max_engine_steps_per_transition,
                uniform_exploration_ppm,
                allow_potion_expenditure: max_potions_used != Some(0),
                allow_potion_discard,
                generation_quantum_work,
                backed_generation_quantum_work: defaults.backed_generation_quantum_work,
                guide_service_bias,
                initial_expansion_work: initial_expansion_work
                    .unwrap_or(defaults.initial_expansion_work),
                root_initial_expansion_work: root_initial_expansion_work_for_budget(
                    max_generation_work,
                ),
                max_turn_depth,
                max_potions_used,
                allowed_potion_slots,
            },
            allowance: LocalTurnGraphSearchAllowanceV1 {
                max_selections,
                max_generation_work,
                max_engine_steps: max_generation_work
                    .saturating_mul(max_engine_steps_per_transition),
                wall_ms,
            },
        }
    }

    pub fn planner_config(
        self,
        satisfaction: OracleCombatWitnessSatisfaction,
    ) -> LocalTurnGraphWitnessConfig {
        LocalTurnGraphWitnessConfig {
            generator: TurnOptionGeneratorConfig {
                max_engine_steps_per_transition: self.planner.max_engine_steps_per_transition,
                uniform_exploration_ppm: self.planner.uniform_exploration_ppm,
                allow_potion_expenditure: self.planner.allow_potion_expenditure,
                allow_potion_discard: self.planner.allow_potion_discard,
                allowed_potion_slots: self.planner.allowed_potion_slots,
            },
            generation_quantum_work: self.planner.generation_quantum_work,
            backed_generation_quantum_work: self.planner.backed_generation_quantum_work,
            guide_service_bias: self.planner.guide_service_bias.map(|bias| {
                CombatGuideServiceBias {
                    lane: CombatGuideLaneId::new(bias.lane),
                    extra_services_per_cycle: bias.extra_services_per_cycle,
                }
            }),
            initial_expansion_work: self.planner.initial_expansion_work,
            root_initial_expansion_work: self.planner.root_initial_expansion_work,
            max_turn_depth: self.planner.max_turn_depth,
            satisfaction,
            require_no_unrecovered_stolen_gold: false,
            max_potions_used: self.planner.max_potions_used,
        }
    }

    /// Lab-facing override for the first coherent grounding batch granted to
    /// a fresh exact node selected by a guide or typed proposal. Production
    /// callers retain the planner default unless they opt in explicitly.
    pub fn with_backed_generation_quantum_work(mut self, work: usize) -> Self {
        self.planner.backed_generation_quantum_work = work;
        self
    }

    pub fn quantum(self) -> LocalTurnGraphWitnessQuantum {
        LocalTurnGraphWitnessQuantum {
            additional_selections: self.allowance.max_selections,
            additional_generation_work: self.allowance.max_generation_work,
            additional_engine_steps: self.allowance.max_engine_steps,
            deadline: Some(Instant::now() + Duration::from_millis(self.allowance.wall_ms)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_serializes_every_effective_setting_and_allowance() {
        let spec = LocalTurnGraphSearchSpecV1::from_controls(
            240,
            80,
            15,
            7,
            12_345,
            3,
            9,
            Some(0),
            false,
            Some(4),
            Some(96),
            Some(LocalTurnGraphGuideServiceBiasSpecV1::new(2, 2)),
        );
        let value = serde_json::to_value(spec).expect("serialize search spec");

        assert_eq!(value["planner"]["max_engine_steps_per_transition"], 7);
        assert_eq!(value["planner"]["uniform_exploration_ppm"], 12_345);
        assert_eq!(value["planner"]["allow_potion_expenditure"], false);
        assert_eq!(value["planner"]["allow_potion_discard"], false);
        assert_eq!(value["planner"]["generation_quantum_work"], 3);
        assert_eq!(
            value["planner"]["backed_generation_quantum_work"],
            crate::DEFAULT_BACKED_GENERATION_QUANTUM_WORK
        );
        assert_eq!(value["planner"]["guide_service_bias"]["lane"], 2);
        assert_eq!(
            value["planner"]["guide_service_bias"]["extra_services_per_cycle"],
            2
        );
        assert_eq!(value["planner"]["root_initial_expansion_work"], 64);
        assert_eq!(value["planner"]["max_turn_depth"], 9);
        assert_eq!(value["planner"]["max_potions_used"], 0);
        assert_eq!(value["planner"]["allowed_potion_slots"], 4);
        assert_eq!(value["planner"]["initial_expansion_work"], 96);
        assert_eq!(value["allowance"]["max_selections"], 80);
        assert_eq!(value["allowance"]["max_generation_work"], 240);
        assert_eq!(value["allowance"]["max_engine_steps"], 1_680);
        assert_eq!(value["allowance"]["wall_ms"], 15);
    }

    #[test]
    fn planner_config_and_quantum_are_built_from_the_reported_spec() {
        let spec = LocalTurnGraphSearchSpecV1::from_controls(
            240,
            80,
            15,
            7,
            12_345,
            3,
            9,
            Some(2),
            true,
            Some(2),
            None,
            Some(LocalTurnGraphGuideServiceBiasSpecV1::new(2, 1)),
        )
        .with_backed_generation_quantum_work(11);
        let config = spec.planner_config(OracleCombatWitnessSatisfaction::HpLossAtMost(4));
        let quantum = spec.quantum();

        assert_eq!(config.generator.max_engine_steps_per_transition, 7);
        assert_eq!(config.generator.uniform_exploration_ppm, 12_345);
        assert!(config.generator.allow_potion_expenditure);
        assert!(config.generator.allow_potion_discard);
        assert_eq!(config.generator.allowed_potion_slots, Some(2));
        assert_eq!(config.generation_quantum_work, 3);
        assert_eq!(config.backed_generation_quantum_work, 11);
        assert_eq!(
            config.guide_service_bias,
            Some(CombatGuideServiceBias {
                lane: CombatGuideLaneId::new(2),
                extra_services_per_cycle: 1,
            })
        );
        assert_eq!(config.root_initial_expansion_work, 64);
        assert_eq!(config.max_turn_depth, 9);
        assert_eq!(config.max_potions_used, Some(2));
        assert_eq!(quantum.additional_selections, 80);
        assert_eq!(quantum.additional_generation_work, 240);
        assert_eq!(quantum.additional_engine_steps, 1_680);
        assert!(quantum.deadline.is_some());
    }
}
