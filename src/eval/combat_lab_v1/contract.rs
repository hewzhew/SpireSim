use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use blake2::{Blake2b512, Digest};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ai::combat_search_v2::{
    CombatSearchV2ChildRolloutPolicy, CombatSearchV2Config, CombatSearchV2PhaseGuardPolicy,
    CombatSearchV2PotionPolicy, CombatSearchV2RolloutPolicy, CombatSearchV2SetupBiasPolicy,
    CombatSearchV2TurnPlanPolicy,
};
use crate::testing::fixtures::combat_start_spec::CombatStartSpec;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLabSpecV1 {
    pub schema_version: u32,
    pub experiment_id: String,
    pub scenario_id: String,
    pub start_spec: PathBuf,
    pub schedule: CombatLabShuffleScheduleV1,
    pub profiles: Vec<CombatLabProfileSpecV1>,
    pub common_budget: CombatLabCommonBudgetV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLabShuffleScheduleV1 {
    pub generator: CombatLabShuffleGeneratorV1,
    pub seed: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLabShuffleGeneratorV1 {
    SplitMix64V1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLabProfileSpecV1 {
    pub id: String,
    pub label: String,
    pub information_scope: CombatLabInformationScopeV1,
    pub potion_policy: CombatSearchV2PotionPolicy,
    pub rollout_policy: CombatSearchV2RolloutPolicy,
    pub child_rollout_policy: CombatSearchV2ChildRolloutPolicy,
    pub turn_plan_policy: CombatSearchV2TurnPlanPolicy,
    pub phase_guard_policy: CombatSearchV2PhaseGuardPolicy,
    pub setup_bias_policy: CombatSearchV2SetupBiasPolicy,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLabInformationScopeV1 {
    ExactStateOracle,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLabCommonBudgetV1 {
    pub max_nodes: usize,
    pub max_actions_per_line: usize,
    pub max_engine_steps_per_action: usize,
    pub wall_ms: Option<u64>,
    #[serde(default)]
    pub satisfaction: crate::ai::combat_search_v2::CombatSearchV2Satisfaction,
    pub max_potions_used: Option<u32>,
    pub rollout_max_evaluations: usize,
    pub rollout_max_actions: usize,
    pub rollout_beam_width: usize,
    pub turn_plan_probe_max_inner_nodes: Option<usize>,
    pub turn_plan_probe_max_end_states: Option<usize>,
    pub turn_plan_probe_per_bucket_limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResolvedCombatLabProfileV1 {
    pub spec: CombatLabProfileSpecV1,
    pub profile_hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResolvedCombatLabSpecV1 {
    pub schema_version: u32,
    pub experiment_id: String,
    pub scenario_id: String,
    pub lab_spec_path: PathBuf,
    pub start_spec_path: PathBuf,
    pub start_spec_snapshot: CombatStartSpec,
    pub schedule: CombatLabShuffleScheduleV1,
    pub profiles: Vec<ResolvedCombatLabProfileV1>,
    pub common_budget: CombatLabCommonBudgetV1,
    pub scenario_hash: String,
    pub budget_hash: String,
    pub experiment_hash: String,
}

#[derive(Serialize)]
struct CombatLabProfileHashInputV1<'a> {
    information_scope: &'a CombatLabInformationScopeV1,
    potion_policy: &'a CombatSearchV2PotionPolicy,
    rollout_policy: &'a CombatSearchV2RolloutPolicy,
    child_rollout_policy: &'a CombatSearchV2ChildRolloutPolicy,
    turn_plan_policy: &'a CombatSearchV2TurnPlanPolicy,
    phase_guard_policy: &'a CombatSearchV2PhaseGuardPolicy,
    setup_bias_policy: &'a CombatSearchV2SetupBiasPolicy,
}

#[derive(Serialize)]
struct CombatLabExperimentProfileHashInputV1<'a> {
    id: &'a str,
    label: &'a str,
    profile_hash: &'a str,
}

#[derive(Serialize)]
struct CombatLabExperimentHashInputV1<'a> {
    schema_version: u32,
    shuffle_generator_version: CombatLabShuffleGeneratorV1,
    experiment_id: &'a str,
    scenario_id: &'a str,
    scenario_hash: &'a str,
    profiles: Vec<CombatLabExperimentProfileHashInputV1<'a>>,
    schedule: &'a CombatLabShuffleScheduleV1,
    budget_hash: &'a str,
}

pub fn load_and_resolve_combat_lab_spec_v1(
    lab_spec_path: &Path,
) -> Result<ResolvedCombatLabSpecV1, String> {
    let lab_spec_path = fs::canonicalize(lab_spec_path).map_err(|error| {
        format!(
            "failed to canonicalize combat laboratory spec '{}': {error}",
            lab_spec_path.display()
        )
    })?;
    let bytes = fs::read(&lab_spec_path).map_err(|error| {
        format!(
            "failed to read combat laboratory spec '{}': {error}",
            lab_spec_path.display()
        )
    })?;
    let spec: CombatLabSpecV1 = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse combat laboratory spec '{}': {error}",
            lab_spec_path.display()
        )
    })?;
    if spec.schema_version != 1 {
        return Err(format!(
            "unsupported combat laboratory schema_version {}; expected 1",
            spec.schema_version
        ));
    }
    if spec.profiles.is_empty() {
        return Err("combat laboratory spec requires at least one profile".to_string());
    }
    let mut profile_ids = HashSet::new();
    for profile in &spec.profiles {
        if !profile_ids.insert(profile.id.as_str()) {
            return Err(format!("duplicate profile id '{}'", profile.id));
        }
    }

    let start_spec_candidate = if spec.start_spec.is_absolute() {
        spec.start_spec.clone()
    } else {
        lab_spec_path
            .parent()
            .expect("canonical lab spec path should have a parent")
            .join(&spec.start_spec)
    };
    let start_spec_path = fs::canonicalize(&start_spec_candidate).map_err(|error| {
        format!(
            "failed to canonicalize combat start spec '{}': {error}",
            start_spec_candidate.display()
        )
    })?;
    let start_spec_bytes = fs::read(&start_spec_path).map_err(|error| {
        format!(
            "failed to read combat start spec '{}': {error}",
            start_spec_path.display()
        )
    })?;
    let start_spec_snapshot: CombatStartSpec =
        serde_json::from_slice(&start_spec_bytes).map_err(|error| {
            format!(
                "failed to parse combat start spec '{}': {error}",
                start_spec_path.display()
            )
        })?;

    let scenario_hash = canonical_hash(&start_spec_snapshot)?;
    let budget_hash = canonical_hash(&spec.common_budget)?;
    let profiles = spec
        .profiles
        .into_iter()
        .map(|profile| {
            let profile_hash = canonical_hash(&CombatLabProfileHashInputV1 {
                information_scope: &profile.information_scope,
                potion_policy: &profile.potion_policy,
                rollout_policy: &profile.rollout_policy,
                child_rollout_policy: &profile.child_rollout_policy,
                turn_plan_policy: &profile.turn_plan_policy,
                phase_guard_policy: &profile.phase_guard_policy,
                setup_bias_policy: &profile.setup_bias_policy,
            })?;
            Ok(ResolvedCombatLabProfileV1 {
                spec: profile,
                profile_hash,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let experiment_hash = canonical_hash(&CombatLabExperimentHashInputV1 {
        schema_version: spec.schema_version,
        shuffle_generator_version: spec.schedule.generator,
        experiment_id: &spec.experiment_id,
        scenario_id: &spec.scenario_id,
        scenario_hash: &scenario_hash,
        profiles: profiles
            .iter()
            .map(|profile| CombatLabExperimentProfileHashInputV1 {
                id: &profile.spec.id,
                label: &profile.spec.label,
                profile_hash: &profile.profile_hash,
            })
            .collect(),
        schedule: &spec.schedule,
        budget_hash: &budget_hash,
    })?;

    Ok(ResolvedCombatLabSpecV1 {
        schema_version: spec.schema_version,
        experiment_id: spec.experiment_id,
        scenario_id: spec.scenario_id,
        lab_spec_path,
        start_spec_path,
        start_spec_snapshot,
        schedule: spec.schedule,
        profiles,
        common_budget: spec.common_budget,
        scenario_hash,
        budget_hash,
        experiment_hash,
    })
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let mut value = serde_json::to_value(value)
        .map_err(|error| format!("failed to serialize combat laboratory hash input: {error}"))?;
    sort_json_object_keys_recursively(&mut value);
    let bytes = serde_json::to_vec(&value).map_err(|error| {
        format!("failed to encode canonical combat laboratory hash input: {error}")
    })?;
    let mut hasher = Blake2b512::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    Ok(digest[..32]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn sort_json_object_keys_recursively(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for value in object.values_mut() {
                sort_json_object_keys_recursively(value);
            }
            let mut entries = std::mem::take(object).into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            for (key, value) in entries {
                object.insert(key, value);
            }
        }
        Value::Array(values) => {
            for value in values {
                sort_json_object_keys_recursively(value);
            }
        }
        _ => {}
    }
}

pub fn derive_shuffle_seed_v1(schedule: &CombatLabShuffleScheduleV1, sample_index: u64) -> u64 {
    const GOLDEN_GAMMA: u64 = 0x9E3779B97F4A7C15;

    match schedule.generator {
        CombatLabShuffleGeneratorV1::SplitMix64V1 => {
            let mut state = schedule
                .seed
                .wrapping_add(GOLDEN_GAMMA.wrapping_mul(sample_index.wrapping_add(1)));
            state = (state ^ (state >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            state = (state ^ (state >> 27)).wrapping_mul(0x94D049BB133111EB);
            state ^ (state >> 31)
        }
    }
}

pub fn profile_config_v1(
    experiment_id: &str,
    profile: &CombatLabProfileSpecV1,
    budget: &CombatLabCommonBudgetV1,
) -> CombatSearchV2Config {
    CombatSearchV2Config {
        max_nodes: budget.max_nodes,
        max_actions_per_line: budget.max_actions_per_line,
        max_engine_steps_per_action: budget.max_engine_steps_per_action,
        wall_time: budget.wall_ms.map(std::time::Duration::from_millis),
        satisfaction: budget.satisfaction,
        input_label: Some(format!("combat_lab_v1/{experiment_id}/{}", profile.id)),
        potion_policy: profile.potion_policy,
        max_potions_used: budget.max_potions_used,
        rollout_policy: profile.rollout_policy,
        child_rollout_policy: profile.child_rollout_policy,
        rollout_max_evaluations: budget.rollout_max_evaluations,
        rollout_max_actions: budget.rollout_max_actions,
        rollout_beam_width: budget.rollout_beam_width,
        turn_plan_policy: profile.turn_plan_policy,
        phase_guard_policy: profile.phase_guard_policy,
        setup_bias_policy: profile.setup_bias_policy,
        turn_plan_probe_max_inner_nodes: budget.turn_plan_probe_max_inner_nodes,
        turn_plan_probe_max_end_states: budget.turn_plan_probe_max_end_states,
        turn_plan_probe_per_bucket_limit: budget.turn_plan_probe_per_bucket_limit,
        root_action_prior: None,
        turn_plan_prior: None,
        ..Default::default()
    }
}
