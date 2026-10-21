//! Public run context carried into combat decisions.

use serde::{Deserialize, Serialize};

use crate::ai::planner_core::{PlannerPublicMap, PlannerRunGoal};
use crate::content::monsters::factory::EncounterId;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PublicCombatRunContextV1 {
    Available {
        run_goal: PlannerRunGoal,
        /// Added after V1 publication; legacy durable observations default to
        /// A0 rather than losing replay compatibility.
        #[serde(default)]
        ascension_level: u8,
        /// Whether winning this combat deterministically restores full HP
        /// before another damage-bearing decision. Legacy durable observations
        /// conservatively preserve HP as a carryover resource.
        #[serde(default, skip_serializing_if = "is_false")]
        combat_victory_guarantees_full_heal: bool,
        act: u8,
        floor: i32,
        keys: [bool; 3],
        public_map: PlannerPublicMap,
        encounter_id: Option<EncounterId>,
    },
    Unavailable {
        reason: PublicCombatRunContextGapV1,
    },
}

impl PublicCombatRunContextV1 {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatRunContextGapV1 {
    DetachedExactCombatPosition,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_available_context_defaults_guaranteed_full_heal_to_false() {
        let legacy = r#"{
                "status":"available",
                "run_goal":"act_three_victory",
                "act":1,
                "floor":16,
                "keys":[false,false,false],
                "public_map":{"current_x":0,"current_y":0,"boss":null,"nodes":[]},
                "encounter_id":null
            }"#;
        let context: PublicCombatRunContextV1 =
            serde_json::from_str(legacy).expect("deserialize legacy public combat run context");

        assert!(matches!(
            &context,
            PublicCombatRunContextV1::Available {
                combat_victory_guarantees_full_heal: false,
                ..
            }
        ));
        assert_eq!(
            serde_json::to_value(context)
                .expect("serialize legacy public combat run context")
                .get("combat_victory_guarantees_full_heal"),
            None,
            "the default must retain the legacy canonical identity shape"
        );
    }
}
