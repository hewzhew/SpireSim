use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sts_combat_planner::{
    CombatActionPolicy, CombatGuideLaneId, CombatPolicyChoice, CombatStateGuide,
    SharedCombatActionPolicy,
};

use super::combat_action_imitation::{
    combat_action_imitation_policy_v1, CombatActionImitationArtifactV1,
};
use crate::sim::combat::CombatPosition;
use crate::sim::combat_action_surface::CombatSelectionActionFamilyV2;
use crate::state::core::ClientInput;
use crate::{content::potions::Potion, content::potions::ALL_POTIONS};

pub const COMBAT_VALUE_PROTOTYPE_SCHEMA_NAME: &str = "CombatValuePrototypeArtifactV1";
pub const COMBAT_VALUE_PROTOTYPE_SCHEMA_VERSION: u32 = 3;
pub const COMBAT_VALUE_FEATURE_SCHEMA: &str =
    "existing-combat-guides-plus-potion-inventory/concatenated-v2";
pub const COMBAT_GUIDANCE_BUNDLE_SCHEMA_NAME: &str = "CombatGuidanceBundleV1";
pub const COMBAT_GUIDANCE_BUNDLE_SCHEMA_VERSION: u32 = 1;

/// A dedicated lane keeps learned cross-turn evidence separate from the
/// handwritten progress/survival/horizon/setup lanes (1..=5).
pub const GUIDE_LEARNED_BOUNDARY_VALUE: CombatGuideLaneId = CombatGuideLaneId::new(7);

const COMBAT_GUIDANCE_RUNTIME_ID: &str = env!("STS_COMBAT_GUIDANCE_RUNTIME_ID");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatValuePrototypeArtifactV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub feature_schema: String,
    /// Fingerprint of the feature extractor used at inference time.
    #[serde(default)]
    pub runtime_compatibility_id: String,
    pub training_authority: String,
    pub source_trajectory_count: usize,
    pub source_action_count: usize,
    pub source_terminal_final_hp: i32,
    pub feature_count: usize,
    pub prototypes: Vec<CombatValuePrototypeV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub one_turn_viability_prototypes: Vec<CombatValueStatePrototypeV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub one_turn_loss_prototypes: Vec<CombatValueStatePrototypeV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatValuePrototypeV1 {
    pub source_trajectory_index: usize,
    pub player_turn: u32,
    pub value_rank: i32,
    pub features: Vec<i32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatValueStatePrototypeV1 {
    pub player_turn: u32,
    pub features: Vec<i32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatGuidanceBundleV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub training_authority: String,
    /// Artifacts are embedded deliberately. A run never depends on mutable
    /// sibling paths that can silently change after the bundle is validated.
    pub action_imitation: CombatActionImitationArtifactV1,
    pub boundary_value: CombatValuePrototypeArtifactV1,
}

impl CombatValuePrototypeArtifactV1 {
    pub fn from_ranked_features(
        training_authority: impl Into<String>,
        source_action_count: usize,
        source_terminal_final_hp: i32,
        prototypes: impl IntoIterator<Item = (u32, i32, Vec<i32>)>,
    ) -> Result<Self, String> {
        Self::from_ranked_feature_trajectories(
            training_authority,
            [(
                source_action_count,
                source_terminal_final_hp,
                prototypes.into_iter().collect(),
            )],
        )
    }

    pub fn from_ranked_feature_trajectories(
        training_authority: impl Into<String>,
        trajectories: impl IntoIterator<Item = (usize, i32, Vec<(u32, i32, Vec<i32>)>)>,
    ) -> Result<Self, String> {
        let trajectories = trajectories.into_iter().collect::<Vec<_>>();
        let source_trajectory_count = trajectories.len();
        let source_action_count = trajectories.iter().fold(0usize, |total, trajectory| {
            total.saturating_add(trajectory.0)
        });
        let source_terminal_final_hp = trajectories
            .iter()
            .map(|trajectory| trajectory.1)
            .min()
            .unwrap_or_default();
        let mut prototypes = trajectories
            .into_iter()
            .enumerate()
            .flat_map(|(source_trajectory_index, (_, _, prototypes))| {
                prototypes
                    .into_iter()
                    .map(
                        move |(player_turn, value_rank, features)| CombatValuePrototypeV1 {
                            source_trajectory_index,
                            player_turn,
                            value_rank,
                            features,
                        },
                    )
            })
            .collect::<Vec<_>>();
        prototypes
            .sort_by_key(|prototype| (prototype.source_trajectory_index, prototype.player_turn));
        let artifact = Self {
            schema_name: COMBAT_VALUE_PROTOTYPE_SCHEMA_NAME.to_string(),
            schema_version: COMBAT_VALUE_PROTOTYPE_SCHEMA_VERSION,
            feature_schema: COMBAT_VALUE_FEATURE_SCHEMA.to_string(),
            runtime_compatibility_id: COMBAT_GUIDANCE_RUNTIME_ID.to_string(),
            training_authority: training_authority.into(),
            source_trajectory_count,
            source_action_count,
            source_terminal_final_hp,
            feature_count: prototypes
                .first()
                .map(|prototype| prototype.features.len())
                .unwrap_or_default(),
            prototypes,
            one_turn_viability_prototypes: Vec::new(),
            one_turn_loss_prototypes: Vec::new(),
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_name != COMBAT_VALUE_PROTOTYPE_SCHEMA_NAME
            || self.schema_version != COMBAT_VALUE_PROTOTYPE_SCHEMA_VERSION
            || self.feature_schema != COMBAT_VALUE_FEATURE_SCHEMA
        {
            return Err(format!(
                "unsupported combat value prototype schema: found {}/{}/{}, expected {}/{}/{}; rebuild the artifact",
                self.schema_name,
                self.schema_version,
                self.feature_schema,
                COMBAT_VALUE_PROTOTYPE_SCHEMA_NAME,
                COMBAT_VALUE_PROTOTYPE_SCHEMA_VERSION,
                COMBAT_VALUE_FEATURE_SCHEMA,
            ));
        }
        if self.runtime_compatibility_id != COMBAT_GUIDANCE_RUNTIME_ID {
            return Err(format!(
                "combat value prototype runtime mismatch: found {:?}, expected {:?}; rebuild the artifact with the current binary",
                self.runtime_compatibility_id, COMBAT_GUIDANCE_RUNTIME_ID,
            ));
        }
        if self.training_authority.is_empty() {
            return Err("combat value prototype training authority is empty".to_string());
        }
        if self.source_trajectory_count == 0
            || self.prototypes.is_empty()
            || self.feature_count == 0
        {
            return Err("combat value prototype artifact is empty".to_string());
        }
        if self
            .prototypes
            .iter()
            .any(|prototype| prototype.features.len() != self.feature_count)
            || self
                .one_turn_loss_prototypes
                .iter()
                .any(|prototype| prototype.features.len() != self.feature_count)
            || self
                .one_turn_viability_prototypes
                .iter()
                .any(|prototype| prototype.features.len() != self.feature_count)
        {
            return Err("combat value prototype feature widths disagree".to_string());
        }
        let source_indices = self
            .prototypes
            .iter()
            .map(|prototype| prototype.source_trajectory_index)
            .collect::<HashSet<_>>();
        if source_indices.len() != self.source_trajectory_count
            || source_indices
                .iter()
                .any(|index| *index >= self.source_trajectory_count)
        {
            return Err(
                "combat value prototype trajectory indices disagree with the source count"
                    .to_string(),
            );
        }
        if self.prototypes.windows(2).any(|pair| {
            pair[0].source_trajectory_index > pair[1].source_trajectory_index
                || (pair[0].source_trajectory_index == pair[1].source_trajectory_index
                    && pair[0].player_turn >= pair[1].player_turn)
        }) {
            return Err(
                "combat value prototypes must have unique ascending turns per trajectory"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, bytes).map_err(|error| error.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let artifact = serde_json::from_slice::<Self>(
            &std::fs::read(path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("invalid combat value prototype artifact: {error}"))?;
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn targets_by_turn(&self) -> HashMap<u32, Vec<(i32, Vec<i32>)>> {
        let mut targets = HashMap::<u32, Vec<(i32, Vec<i32>)>>::new();
        for prototype in &self.prototypes {
            targets
                .entry(prototype.player_turn)
                .or_default()
                .push((prototype.value_rank, prototype.features.clone()));
        }
        targets
    }

    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": "typed_feature_value_prototype",
            "authority": "guide_only",
            "feature_schema": self.feature_schema,
            "runtime_compatibility_id": self.runtime_compatibility_id,
            "training_authority": self.training_authority,
            "source_trajectory_count": self.source_trajectory_count,
            "feature_count": self.feature_count,
            "prototype_count": self.prototypes.len(),
            "one_turn_viability_prototype_count": self.one_turn_viability_prototypes.len(),
            "one_turn_viability_prototype_authority": "training_evidence_only",
            "one_turn_loss_prototype_count": self.one_turn_loss_prototypes.len(),
            "one_turn_loss_prototype_authority": "training_evidence_only",
            "source_action_count": self.source_action_count,
            "source_terminal_final_hp": self.source_terminal_final_hp,
            "runtime_reads_exact_hashes": false,
            "runtime_reads_witness_actions": false,
        })
    }

    pub fn add_one_turn_viability_positions<'a>(
        &mut self,
        positions: impl IntoIterator<Item = &'a CombatPosition>,
    ) {
        let mut known = self
            .one_turn_viability_prototypes
            .iter()
            .map(|prototype| (prototype.player_turn, prototype.features.clone()))
            .collect::<HashSet<_>>();
        for position in positions {
            let player_turn = position.combat.turn.turn_count;
            let features = typed_combat_value_features_v1(position);
            if known.insert((player_turn, features.clone())) {
                self.one_turn_viability_prototypes
                    .push(CombatValueStatePrototypeV1 {
                        player_turn,
                        features,
                    });
            }
        }
        self.one_turn_viability_prototypes
            .sort_by_key(|prototype| prototype.player_turn);
    }

    pub fn add_one_turn_loss_positions<'a>(
        &mut self,
        positions: impl IntoIterator<Item = &'a CombatPosition>,
    ) {
        let mut known = self
            .one_turn_loss_prototypes
            .iter()
            .map(|prototype| (prototype.player_turn, prototype.features.clone()))
            .collect::<HashSet<_>>();
        for position in positions {
            let player_turn = position.combat.turn.turn_count;
            let features = typed_combat_value_features_v1(position);
            if known.insert((player_turn, features.clone())) {
                self.one_turn_loss_prototypes
                    .push(CombatValueStatePrototypeV1 {
                        player_turn,
                        features,
                    });
            }
        }
        self.one_turn_loss_prototypes
            .sort_by_key(|prototype| prototype.player_turn);
    }
}

impl CombatGuidanceBundleV1 {
    pub fn new(
        training_authority: impl Into<String>,
        action_imitation: CombatActionImitationArtifactV1,
        boundary_value: CombatValuePrototypeArtifactV1,
    ) -> Result<Self, String> {
        let bundle = Self {
            schema_name: COMBAT_GUIDANCE_BUNDLE_SCHEMA_NAME.to_string(),
            schema_version: COMBAT_GUIDANCE_BUNDLE_SCHEMA_VERSION,
            training_authority: training_authority.into(),
            action_imitation,
            boundary_value,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_name != COMBAT_GUIDANCE_BUNDLE_SCHEMA_NAME
            || self.schema_version != COMBAT_GUIDANCE_BUNDLE_SCHEMA_VERSION
        {
            return Err("unsupported combat guidance bundle schema".to_string());
        }
        if self.training_authority.is_empty() {
            return Err("combat guidance bundle training authority is empty".to_string());
        }
        self.action_imitation.validate()?;
        self.boundary_value.validate()?;
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, bytes).map_err(|error| error.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let bundle = serde_json::from_slice::<Self>(
            &std::fs::read(path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("invalid combat guidance bundle: {error}"))?;
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn policy(
        &self,
        base: SharedCombatActionPolicy,
    ) -> Result<SharedCombatActionPolicy, String> {
        let action_policy = combat_action_imitation_policy_v1(base, self.action_imitation.clone())?;
        Ok(combat_value_prototype_policy_v1(
            action_policy,
            &self.boundary_value,
        ))
    }
}

#[derive(Clone)]
struct CombatValuePrototypePolicyV1 {
    base: SharedCombatActionPolicy,
    typed_target_by_turn: Arc<HashMap<u32, Vec<(i32, Vec<i32>)>>>,
}

impl CombatActionPolicy for CombatValuePrototypePolicyV1 {
    fn weights(&self, position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        self.base.weights(position, choices)
    }

    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
    ) -> sts_combat_planner::CombatSelectionPreferences {
        self.base.structured_selection_preferences(position, family)
    }

    fn structured_selection_member_weights(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        self.base
            .structured_selection_member_weights(position, family, members)
    }

    fn state_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        let mut guides = self.base.state_guides(position);
        let rank = combat_value_prototype_rank_v1(
            &self.typed_target_by_turn,
            position,
            position.combat.turn.turn_count,
        );
        guides.push(CombatStateGuide::new(GUIDE_LEARNED_BOUNDARY_VALUE, rank));
        guides
    }

    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        // The artifact is cross-turn evidence. It must not pretend to evaluate
        // partial action sequences inside the current turn.
        self.base.turn_generation_guides(position)
    }
}

pub fn combat_value_prototype_policy_v1(
    base: SharedCombatActionPolicy,
    artifact: &CombatValuePrototypeArtifactV1,
) -> SharedCombatActionPolicy {
    Arc::new(CombatValuePrototypePolicyV1 {
        base,
        typed_target_by_turn: Arc::new(artifact.targets_by_turn()),
    })
}

pub fn typed_combat_value_features_v1(position: &CombatPosition) -> Vec<i32> {
    let mut features =
        crate::ai::combat_search_v2::oracle_action_policy::oracle_combat_state_guide_components(
            position,
        );
    features.extend(
        crate::ai::combat_search_v2::oracle_action_policy::oracle_combat_survival_guide_components(
            position,
        ),
    );
    features.extend(
        crate::ai::combat_search_v2::oracle_action_policy::oracle_combat_horizon_guide_components(
            position,
        ),
    );
    features.extend(
        crate::ai::combat_search_v2::oracle_action_policy::oracle_combat_setup_guide_components(
            position,
        ),
    );
    features.extend(potion_inventory_features(&position.combat.entities.potions));
    features
}

fn potion_inventory_features(potions: &[Option<Potion>]) -> Vec<i32> {
    ALL_POTIONS
        .iter()
        .map(|id| {
            potions
                .iter()
                .flatten()
                .filter(|potion| potion.id == *id)
                .count() as i32
        })
        .collect()
}

pub fn combat_value_prototype_rank_v1(
    targets_by_turn: &HashMap<u32, Vec<(i32, Vec<i32>)>>,
    position: &CombatPosition,
    target_turn: u32,
) -> Vec<i32> {
    let Some(targets) = targets_by_turn.get(&target_turn) else {
        return vec![0, i32::MIN / 4, 0];
    };
    let candidate = typed_combat_value_features_v1(position);
    let best = targets
        .iter()
        .map(|(corridor_rank, target)| {
            (
                normalized_feature_distance(target, &candidate),
                *corridor_rank,
            )
        })
        .min_by_key(|(distance, corridor_rank)| (*distance, std::cmp::Reverse(*corridor_rank)))
        .expect("a value prototype turn group is never empty");
    vec![i32::from(best.0 == 0), -best.0, best.1]
}

fn normalized_feature_distance(target: &[i32], candidate: &[i32]) -> i32 {
    let distance = target
        .iter()
        .zip(candidate)
        .map(|(target, candidate)| {
            let difference = i64::from(*target).abs_diff(i64::from(*candidate)) as i64;
            let scale = i64::from(*target)
                .abs()
                .max(i64::from(*candidate).abs())
                .max(1);
            difference.saturating_mul(1_024) / scale
        })
        .fold(0_i64, i64::saturating_add);
    i32::try_from(distance).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::potions::{Potion, PotionId};

    #[test]
    fn bundle_rejects_empty_authority_before_runtime_use() {
        let value = CombatValuePrototypeArtifactV1 {
            schema_name: COMBAT_VALUE_PROTOTYPE_SCHEMA_NAME.to_string(),
            schema_version: COMBAT_VALUE_PROTOTYPE_SCHEMA_VERSION,
            feature_schema: COMBAT_VALUE_FEATURE_SCHEMA.to_string(),
            runtime_compatibility_id: COMBAT_GUIDANCE_RUNTIME_ID.to_string(),
            training_authority: String::new(),
            source_trajectory_count: 1,
            source_action_count: 1,
            source_terminal_final_hp: 1,
            feature_count: 1,
            prototypes: vec![CombatValuePrototypeV1 {
                source_trajectory_index: 0,
                player_turn: 1,
                value_rank: 0,
                features: vec![0],
            }],
            one_turn_viability_prototypes: Vec::new(),
            one_turn_loss_prototypes: Vec::new(),
        };
        assert!(value.validate().is_err());
    }

    #[test]
    fn potion_inventory_features_preserve_resource_identity() {
        let with_ancient =
            potion_inventory_features(&[Some(Potion::new(PotionId::AncientPotion, 1)), None]);
        let empty = potion_inventory_features(&[None, None]);

        assert_eq!(with_ancient.len(), ALL_POTIONS.len());
        assert_ne!(with_ancient, empty);
        assert_eq!(with_ancient.iter().sum::<i32>(), 1);
        assert_eq!(empty.iter().sum::<i32>(), 0);
    }
}
