use crate::{
    act, continue_run_with_content_profile, publish_json_create_new, sha256_file,
    start_run_with_content_profile, status, submit_grid_select, submit_hand_select,
};
use crate::{ActionHandle, ActionReceipt, LiveStatus, SessionManifest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sts_java_parity_protocol::{canonical_game_id, game_ids_match};

const MIRROR_PLAN_SCHEMA_V2: &str = "sts-java-parity-mirror-plan-v2";
const MIRROR_PLAN_SCHEMA_V3: &str = "sts-java-parity-mirror-plan-v3";
const MIRROR_PLAN_SCHEMA_V4: &str = "sts-java-parity-mirror-plan-v4";
const MIRROR_SCOPE_FIRST_REWARD: &str = "first-reward";
const MIRROR_SCOPE_ACTIVE_PREFIX: &str = "active-prefix";
const RUN_PLAY_HEAD_SCHEMA_V1: &str = "sts-run-play-head-v1";
const COMBAT_BOUNDARY_PLAYER_DECISION: &str = "player_decision";
const COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD: &str = "toolbox_card_reward";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorPlan {
    pub schema: String,
    pub source: MirrorSource,
    pub run_start: MirrorRunStart,
    pub scope: MirrorScope,
    pub lineage: MirrorLineageAudit,
    pub steps: Vec<MirrorStep>,
    pub coverage: MirrorCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorSource {
    pub head_path: String,
    pub head_sha256: String,
    /// Immutable run-play evidence contract. Missing identifies a legacy,
    /// unscoped Rust root and must never be inferred as current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_play_evidence_identity: Option<Value>,
    pub session_id: String,
    pub head_revision: u64,
    pub state_revision: u64,
    pub outcome: Value,
    pub total_active_path_steps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorRunStart {
    /// Decimal unsigned bit pattern used as `Settings.seed` by the simulator.
    /// Java's public seed field is a base-35 display token, so mirror execution
    /// must encode this value before submitting `start_run`.
    pub seed: String,
    pub character: String,
    pub ascension: u32,
    pub content_profile: String,
    pub objective: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorScope {
    pub kind: String,
    pub first_combat_start_depth: usize,
    pub first_combat_end_depth: usize,
    pub first_reward_end_depth: usize,
    pub expected_successor_site: String,
    /// Exact Java boundary expected when the Rust endpoint is combat. Older
    /// V4 plans omitted this field and therefore retain the historical
    /// `player_decision` interpretation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_successor_combat_boundary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_successor_monster_ids: Option<Vec<String>>,
    /// Additive V4 endpoint evidence. Legacy V4 plans may omit these fields;
    /// newly compiled combat prefixes bind the exact retained Rust decision
    /// that Java must reach after the final planned action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_successor_scalars: Option<MirrorPublicScalars>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_successor_combat: Option<MirrorCombatProjection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorLineageAudit {
    pub active_path_depth_contiguous: bool,
    pub starts_at_neow: bool,
    pub terminal_heart_victory: bool,
    pub controller_event_counts: BTreeMap<String, usize>,
    pub navigation_jump_count: usize,
    pub active_path_selected_independently_of_navigation_events: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorStep {
    pub depth: usize,
    pub site: String,
    pub before: MirrorPublicScalars,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_before: Option<MirrorCombatProjection>,
    pub semantic_action: String,
    pub candidate_domain: String,
    pub action_kind: Option<String>,
    pub selected_candidate: Value,
    #[serde(default)]
    pub choice: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_selection: Option<MirrorRunSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MirrorRunSelection {
    pub source_event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_kind: Option<String>,
    pub scope: String,
    pub selected_card_ordinal: usize,
    pub card_id: String,
    pub upgrades: u8,
    pub occurrence: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorCombatProjection {
    pub turn_index: u32,
    pub energy: u8,
    pub player_block: i32,
    pub hand: Vec<MirrorCombatCard>,
    pub potions: Vec<Option<String>>,
    pub player_powers: Vec<MirrorCombatPower>,
    pub monsters: Vec<MirrorCombatMonster>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorCombatCard {
    pub card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MirrorCombatPower {
    pub power_id: String,
    pub amount: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorCombatMonster {
    pub source_index: usize,
    pub source_slot: u8,
    pub enemy_id: String,
    pub same_enemy_ordinal: usize,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub intent: Option<String>,
    pub powers: Vec<MirrorCombatPower>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorPublicScalars {
    pub act: i64,
    pub floor: i64,
    pub current_hp: i64,
    pub max_hp: i64,
    pub gold: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorCoverage {
    pub required_action_kinds: Vec<String>,
    pub selected_scope_supported_by_current_run_profile: bool,
    pub first_step_after_scope: MirrorBoundaryStep,
    pub first_current_profile_gap: Option<MirrorGap>,
    pub remaining_site_action_counts: Vec<MirrorSiteActionCount>,
    pub rust_fingerprint_source: String,
    pub next_requirement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorBoundaryStep {
    pub origin: String,
    pub archive_depth: usize,
    pub site: String,
    pub action_kind: Option<String>,
    pub semantic_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorGap {
    pub kind: String,
    pub depth: usize,
    pub site: String,
    pub action_kind: Option<String>,
    pub semantic_action: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorSiteActionCount {
    pub site: String,
    pub action_kind: Option<String>,
    pub count: usize,
    pub first_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorExecutionReport {
    pub schema: String,
    pub provenance: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_play_evidence_identity: Option<Value>,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub failure: Option<String>,
    pub plan_path: String,
    pub plan_sha256: String,
    pub expected_launch_plan: String,
    pub session: SessionManifest,
    pub initial_frame: MirrorFileIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<MirrorResumeEvidence>,
    pub planned_steps: usize,
    pub completed_plan_steps: usize,
    pub steps: Vec<MirrorExecutionStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successor_scalars: Option<MirrorScalarComparison>,
    pub first_mismatch: Option<MirrorMismatch>,
    pub evidence_scope: MirrorExecutionEvidenceScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorResumeEvidence {
    pub checkpoint_manifest_path: String,
    pub checkpoint_manifest_sha256: String,
    pub source_depth: usize,
    pub continuous_from_neow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorSuffixBatchPlan {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub checkpoint_manifest: MirrorFileIdentity,
    pub source_depth: usize,
    pub members: Vec<MirrorSuffixBatchMember>,
    pub evidence_scope: MirrorSuffixBatchEvidenceScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorSuffixBatchMember {
    pub member_id: String,
    pub mirror_plan: MirrorFileIdentity,
    pub suffix_steps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorSuffixBatchEvidenceScope {
    pub continuous_from_neow: bool,
    pub simulator_comparison: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeCombatCheckpoint {
    schema: String,
    provenance: String,
    native_scope: String,
    storage_edited: bool,
    source_depth: usize,
    source_plan: NativeCheckpointFileIdentity,
    source_execution: NativeCheckpointFileIdentity,
    source_lifecycle: NativeCheckpointFileIdentity,
    #[serde(default)]
    parent_checkpoint: Option<NativeCheckpointFileIdentity>,
    root_frame: NativeCheckpointFileIdentity,
    autosave: NativeCheckpointFileIdentity,
    storage: Value,
    autosave_summary: Value,
    root_decision: Value,
    evidence_scope: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeCheckpointFileIdentity {
    path: String,
    length: u64,
    sha256: String,
}

/// Read-only view returned only after the existing native checkpoint owner has
/// revalidated every bound file, lifecycle receipt, natural typed prefix and
/// combat-root contract. Paired-root importers consume this view instead of
/// reparsing the checkpoint manifest with a second, weaker validator.
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidatedNativeCombatCheckpointV1 {
    pub manifest: MirrorFileIdentity,
    pub checkpoint_directory: String,
    pub provenance: String,
    pub source_depth: usize,
    pub content_profile: String,
    pub source_plan: MirrorFileIdentity,
    pub source_execution: MirrorFileIdentity,
    pub source_lifecycle: MirrorFileIdentity,
    pub parent_checkpoint: Option<MirrorFileIdentity>,
    pub root_frame: MirrorFileIdentity,
    pub autosave: MirrorFileIdentity,
    pub captured_root_frame: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorFileIdentity {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorExecutionStep {
    pub source_depth: Option<usize>,
    pub semantic_action: String,
    pub frame_id: u64,
    pub action_id: String,
    pub successor_frame_id: u64,
    pub receipt: MirrorFileIdentity,
    pub before_scalars: Option<MirrorScalarComparison>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorScalarComparison {
    pub act: MirrorScalarField,
    pub floor: MirrorScalarField,
    pub current_hp: MirrorScalarField,
    pub max_hp: MirrorScalarField,
    pub gold: MirrorScalarField,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorScalarField {
    pub status: String,
    pub expected: i64,
    pub actual: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorMismatch {
    pub kind: String,
    pub source_depth: Option<usize>,
    pub semantic_action: Option<String>,
    pub frame_id: Option<u64>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorExecutionEvidenceScope {
    pub original_game_prefix_captured: bool,
    pub planned_prefix_completed: bool,
    pub expected_successor_reached: bool,
    pub first_combat_victory_reached: bool,
    pub first_reward_completed: bool,
    pub post_reward_map_reached: bool,
    pub scalar_pilot_compared: bool,
    #[serde(default)]
    pub native_checkpoint_restored: bool,
    #[serde(default)]
    pub checkpoint_suffix_captured: bool,
    #[serde(default)]
    pub continuous_from_neow: bool,
    #[serde(default)]
    pub successor_combat_projection_compared: bool,
    pub complete_rust_semantic_fingerprint_compared: bool,
    pub simulator_comparison: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
}

pub fn compile_mirror_plan(
    head_path: &Path,
    output_path: &Path,
    seed: &str,
    character: &str,
    ascension: u32,
) -> Result<MirrorPlan, String> {
    compile_mirror_plan_with_scope(
        head_path,
        output_path,
        seed,
        character,
        ascension,
        MIRROR_SCOPE_FIRST_REWARD,
    )
}

pub fn compile_mirror_plan_with_scope(
    head_path: &Path,
    output_path: &Path,
    seed: &str,
    character: &str,
    ascension: u32,
    requested_scope: &str,
) -> Result<MirrorPlan, String> {
    java_seed_token_for_engine_seed(seed)?;
    if character.trim().is_empty() {
        return Err("mirror run character cannot be empty".to_owned());
    }
    if ascension > 20 {
        return Err("mirror run ascension must be in 0..=20".to_owned());
    }

    let bytes = fs::read(head_path).map_err(|error| {
        format!(
            "failed to read run-play head {}: {error}",
            head_path.display()
        )
    })?;
    let head: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse run-play head {}: {error}",
            head_path.display()
        )
    })?;
    if head["schema"].as_str() != Some(RUN_PLAY_HEAD_SCHEMA_V1) {
        return Err("mirror source is not an sts-run-play-head-v1 document".to_owned());
    }
    let outcome = head["position"]["outcome"].clone();
    let terminal_heart_victory = head["position"]["kind"].as_str() == Some("terminal")
        && outcome["status"].as_str() == Some("success")
        && outcome["game_outcome"].as_str() == Some("heart_victory");
    let path = head["current_path"]
        .as_array()
        .ok_or_else(|| "run-play head omitted current_path".to_owned())?;
    if path.is_empty() {
        return Err("run-play head has an empty active path".to_owned());
    }
    for (index, step) in path.iter().enumerate() {
        let expected_depth = index + 1;
        if step["depth"].as_u64() != Some(expected_depth as u64) {
            return Err(format!(
                "run-play active path is not depth-contiguous at index {index}"
            ));
        }
        if step["semantic_action"].as_str().is_none_or(str::is_empty) {
            return Err(format!(
                "run-play active path depth {expected_depth} omitted semantic_action"
            ));
        }
        if !step["selected_candidate"].is_object() {
            return Err(format!(
                "run-play active path depth {expected_depth} omitted selected_candidate"
            ));
        }
    }
    if path[0]["site"].as_str() != Some("neow") {
        return Err("mirror source active path does not start at Neow".to_owned());
    }

    let first_combat_index = path
        .iter()
        .position(|step| step["site"].as_str() == Some("combat"))
        .ok_or_else(|| "mirror source active path contains no combat".to_owned())?;
    let first_after_combat_index = path
        .iter()
        .enumerate()
        .skip(first_combat_index)
        .find_map(|(index, step)| (step["site"].as_str() != Some("combat")).then_some(index));
    let first_reward_index = first_after_combat_index
        .ok_or_else(|| "mirror source has not retained its first-combat reward".to_owned())?;
    if path[first_reward_index]["site"].as_str() != Some("reward") {
        return Err("first combat does not have a retained reward successor".to_owned());
    }
    let first_post_reward_map_index = path
        .iter()
        .enumerate()
        .skip(first_reward_index + 1)
        .find_map(|(index, step)| (step["site"].as_str() == Some("map")).then_some(index));
    let (first_reward_scope_end, first_step_after_first_reward) = match first_post_reward_map_index
    {
        Some(index) => (index, boundary_step(&path[index])?),
        None if head["current_decision"]["site"].as_str() == Some("map") => (
            path.len(),
            MirrorBoundaryStep {
                origin: "current_decision".to_owned(),
                archive_depth: path.len(),
                site: "map".to_owned(),
                action_kind: None,
                semantic_action: None,
            },
        ),
        None => {
            return Err(
                "mirror source has not completed its first reward into the post-reward map"
                    .to_owned(),
            )
        }
    };

    let current_site = head["current_decision"]["site"]
        .as_str()
        .filter(|value| !value.is_empty());
    let (plan_schema, scope_kind, scope_end, first_step_after_scope, expected_successor_site) =
        match requested_scope {
            MIRROR_SCOPE_FIRST_REWARD => (
                MIRROR_PLAN_SCHEMA_V4,
                "neow_through_first_reward_to_map",
                first_reward_scope_end,
                first_step_after_first_reward,
                "map".to_owned(),
            ),
            MIRROR_SCOPE_ACTIVE_PREFIX => {
                let expected_site = active_prefix_expected_successor_site(
                    current_site,
                    terminal_heart_victory,
                )?;
                let boundary_origin = if terminal_heart_victory {
                    "terminal_outcome"
                } else {
                    "current_decision"
                };
                (
                    MIRROR_PLAN_SCHEMA_V4,
                    "neow_through_active_prefix",
                    path.len(),
                    MirrorBoundaryStep {
                        origin: boundary_origin.to_owned(),
                        archive_depth: path.len(),
                        site: expected_site.clone(),
                        action_kind: None,
                        semantic_action: None,
                    },
                    expected_site,
                )
            }
            other => {
                return Err(format!(
                    "unknown mirror scope {other:?}; expected {MIRROR_SCOPE_FIRST_REWARD:?} or {MIRROR_SCOPE_ACTIVE_PREFIX:?}"
                ))
            }
        };

    let all_steps = path
        .iter()
        .enumerate()
        .map(|(index, value)| mirror_step(value, index.checked_sub(1).map(|i| &path[i])))
        .collect::<Result<Vec<_>, _>>()?;
    let steps = all_steps[..scope_end].to_vec();
    let expected_successor_monster_ids = if expected_successor_site == "combat" {
        Some(current_combat_monster_ids(&head)?)
    } else {
        None
    };
    let expected_successor_combat_boundary =
        classify_expected_successor_combat_boundary(&head, &expected_successor_site)?;
    let (expected_successor_scalars, expected_successor_combat) =
        if expected_successor_site == "combat" {
            let (scalars, combat) = current_combat_successor_projection(&head)?;
            (Some(scalars), Some(combat))
        } else {
            (None, None)
        };
    let required_action_kinds = steps
        .iter()
        .filter_map(|step| step.action_kind.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let selected_scope_supported_by_current_run_profile =
        steps.iter().all(current_run_profile_supports_step);
    if !selected_scope_supported_by_current_run_profile {
        let gap = steps
            .iter()
            .find(|step| !current_run_profile_supports_step(step))
            .ok_or_else(|| "mirror support accounting disagreed with selected scope".to_owned())?;
        return Err(format!(
            "mirror scope exceeds the current Java run profile at depth {} site={} action_kind={:?}",
            gap.depth, gap.site, gap.action_kind,
        ));
    }

    let first_current_profile_gap = all_steps
        .iter()
        .skip(scope_end)
        .find(|step| !current_run_profile_supports_step(step))
        .map(|step| {
            Ok::<MirrorGap, String>(MirrorGap {
                kind: "unsupported_current_java_run_profile".to_owned(),
                depth: step.depth,
                site: step.site.clone(),
                action_kind: step.action_kind.clone(),
                semantic_action: step.semantic_action.clone(),
                detail: "The retained Rust victory path reaches a typed decision that the current finite Java run-control profile does not authorize.".to_owned(),
            })
        })
        .transpose()?;

    let mut remaining_counts = BTreeMap::<(String, Option<String>), (usize, usize)>::new();
    for step in all_steps.iter().skip(scope_end) {
        let site = step.site.clone();
        let action_kind = step.action_kind.clone();
        let depth = step.depth;
        let entry = remaining_counts
            .entry((site, action_kind))
            .or_insert((0, depth));
        entry.0 += 1;
    }
    let remaining_site_action_counts = remaining_counts
        .into_iter()
        .map(
            |((site, action_kind), (count, first_depth))| MirrorSiteActionCount {
                site,
                action_kind,
                count,
                first_depth,
            },
        )
        .collect();

    let mut controller_event_counts = BTreeMap::new();
    let mut navigation_jump_count = 0usize;
    if let Some(events) = head["events"].as_array() {
        for event in events {
            let kind = event["kind"].as_str().unwrap_or("unknown").to_owned();
            *controller_event_counts.entry(kind.clone()).or_insert(0) += 1;
            if kind == "jump" {
                navigation_jump_count += 1;
            }
        }
    }

    let plan = MirrorPlan {
        schema: plan_schema.to_owned(),
        source: MirrorSource {
            head_path: absolute_string(head_path)?,
            head_sha256: sha256_file(head_path)?,
            run_play_evidence_identity: head
                .get("evidence_identity")
                .filter(|identity| !identity.is_null())
                .cloned(),
            session_id: required_string(&head, "session_id")?,
            head_revision: required_u64(&head, "head_revision")?,
            state_revision: required_u64(&head, "state_revision")?,
            outcome,
            total_active_path_steps: path.len(),
        },
        run_start: MirrorRunStart {
            seed: seed.to_owned(),
            character: character.to_owned(),
            ascension,
            content_profile: "all_unlocked".to_owned(),
            objective: "heart_victory".to_owned(),
        },
        scope: MirrorScope {
            kind: scope_kind.to_owned(),
            first_combat_start_depth: first_combat_index + 1,
            first_combat_end_depth: first_reward_index,
            first_reward_end_depth: first_reward_scope_end,
            expected_successor_site,
            expected_successor_combat_boundary,
            expected_successor_monster_ids,
            expected_successor_scalars,
            expected_successor_combat,
        },
        lineage: MirrorLineageAudit {
            active_path_depth_contiguous: true,
            starts_at_neow: true,
            terminal_heart_victory,
            controller_event_counts,
            navigation_jump_count,
            active_path_selected_independently_of_navigation_events: true,
        },
        steps,
        coverage: MirrorCoverage {
            required_action_kinds,
            selected_scope_supported_by_current_run_profile,
            first_step_after_scope,
            first_current_profile_gap,
            remaining_site_action_counts,
            rust_fingerprint_source: "not_embedded_in_head; replay_or_archive_projection_required".to_owned(),
            next_requirement: "Execute the selected continuous prefix against one fresh Java run, resolve every step from the current Java action space, normalize only typed Java-only continuation actions, and stop at the first typed action or semantic-fingerprint mismatch.".to_owned(),
        },
    };
    publish_json_create_new(output_path, &plan)?;
    Ok(plan)
}

fn active_prefix_expected_successor_site(
    current_site: Option<&str>,
    terminal_heart_victory: bool,
) -> Result<String, String> {
    if terminal_heart_victory {
        return Ok("terminal".to_owned());
    }
    current_site.map(str::to_owned).ok_or_else(|| {
        "active-prefix mirror source must end at a retained current decision or an exact HeartVictory terminal"
            .to_owned()
    })
}

fn mirror_step(value: &Value, previous: Option<&Value>) -> Result<MirrorStep, String> {
    let before = &value["before"];
    let site = required_string(value, "site")?;
    let combat_before = value
        .get("combat_before")
        .filter(|projection| !projection.is_null())
        .map(|projection| {
            serde_json::from_value::<MirrorCombatProjection>(projection.clone()).map_err(|error| {
                format!("mirror source combat_before does not satisfy its typed contract: {error}")
            })
        })
        .transpose()?;
    if site == "combat" && combat_before.is_none() {
        return Err(format!(
            "mirror source combat step at depth {} lacks the typed combat_before projection; rebuild the run-play head with the current workbench",
            required_usize(value, "depth")?
        ));
    }
    Ok(MirrorStep {
        depth: required_usize(value, "depth")?,
        site,
        before: MirrorPublicScalars {
            act: required_i64(before, "act")?,
            floor: required_i64(before, "floor")?,
            current_hp: required_i64(before, "current_hp")?,
            max_hp: required_i64(before, "max_hp")?,
            gold: required_i64(before, "gold")?,
        },
        combat_before,
        semantic_action: required_string(value, "semantic_action")?,
        candidate_domain: required_string(&value["selected_candidate"], "domain")?,
        action_kind: retained_action_kind(value),
        selected_candidate: value["selected_candidate"].clone(),
        choice: value["choice"].clone(),
        run_selection: compile_event_run_selection(value, previous)?,
    })
}

fn compile_event_run_selection(
    value: &Value,
    previous: Option<&Value>,
) -> Result<Option<MirrorRunSelection>, String> {
    if value["site"].as_str() != Some("run_choice")
        || retained_action_kind(value).as_deref() != Some("submit_run_selection")
    {
        return Ok(None);
    }
    let Some(previous) = previous else {
        return Ok(None);
    };
    let previous_action = &previous["selected_candidate"]["action"];
    let transition = &previous_action["public_semantics"]["structured_semantics"]["transition"];
    let source_event = previous_action["event"].as_str().unwrap_or_default();
    let selection_kind = transition["selection"].as_str().unwrap_or_default();
    if previous["site"].as_str() != Some("event")
        || previous_action["kind"].as_str() != Some("choose_event_option")
        || transition["kind"].as_str() != Some("open_selection")
        || source_event.is_empty()
        || selection_kind.is_empty()
    {
        return Ok(None);
    }
    let action = &value["selected_candidate"]["action"];
    let Some(ordinals) = action["selected_card_ordinals"].as_array() else {
        return Ok(None);
    };
    let [ordinal] = ordinals.as_slice() else {
        return Ok(None);
    };
    if action["scope"].as_str() != Some("Deck") {
        return Ok(None);
    }
    let selected_card_ordinal = ordinal
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "event selected deck ordinal is invalid".to_owned())?;
    let semantic_action = required_string(value, "semantic_action")?;
    let (card_id, upgrades, occurrence) = parse_single_card_selection(&semantic_action)?;
    Ok(Some(MirrorRunSelection {
        source_event: source_event.to_owned(),
        selection_kind: Some(selection_kind.to_owned()),
        scope: "Deck".to_owned(),
        selected_card_ordinal,
        card_id,
        upgrades,
        occurrence,
    }))
}

fn parse_single_card_selection(semantic_action: &str) -> Result<(String, u8, usize), String> {
    let payload = semantic_action
        .strip_prefix("select:")
        .filter(|payload| !payload.is_empty() && !payload.contains(','))
        .ok_or_else(|| {
            "event mirror capability requires one stable select:<card>+<upgrades>#<occurrence> action"
                .to_owned()
        })?;
    let (card_and_upgrades, occurrence) = payload
        .rsplit_once('#')
        .ok_or_else(|| "event card selector omitted occurrence".to_owned())?;
    let (card_id, upgrades) = card_and_upgrades
        .rsplit_once('+')
        .ok_or_else(|| "event card selector omitted upgrades".to_owned())?;
    if card_id.is_empty() {
        return Err("event card selector omitted card ID".to_owned());
    }
    let upgrades = upgrades
        .parse::<u8>()
        .map_err(|_| "event card selector upgrades are invalid".to_owned())?;
    let occurrence = occurrence
        .parse::<usize>()
        .map_err(|_| "event card selector occurrence is invalid".to_owned())?;
    Ok((card_id.to_owned(), upgrades, occurrence))
}

fn boundary_step(value: &Value) -> Result<MirrorBoundaryStep, String> {
    Ok(MirrorBoundaryStep {
        origin: "retained_action".to_owned(),
        archive_depth: required_usize(value, "depth")?,
        site: required_string(value, "site")?,
        action_kind: retained_action_kind(value),
        semantic_action: Some(required_string(value, "semantic_action")?),
    })
}

fn current_run_profile_supports(site: &str, action_kind: Option<&str>, domain: &str) -> bool {
    matches!(
        (site, action_kind, domain),
        ("neow" | "event", Some("choose_event_option"), "strategic")
            | ("map", Some("choose_route_node"), "strategic")
            | (
                "reward",
                Some("claim_reward" | "open_card_reward" | "proceed" | "cancel"),
                "strategic",
            )
            | (
                "card_reward",
                Some("take_card" | "skip_card_reward"),
                "strategic"
            )
            | (
                "boss_relic",
                Some("take_boss_relic" | "skip_boss_relic"),
                "strategic"
            )
            | (
                "shop",
                Some("buy_card" | "buy_relic" | "buy_potion" | "remove_card" | "leave_shop"),
                "strategic",
            )
            | ("campfire", Some("rest" | "recall" | "smith"), "strategic")
            | ("treasure", Some("open_chest"), "strategic")
            | (
                "neow"
                    | "event"
                    | "map"
                    | "reward"
                    | "card_reward"
                    | "boss_relic"
                    | "shop"
                    | "campfire"
                    | "treasure",
                Some("discard_run_potion"),
                "strategic",
            )
            | (
                "combat",
                Some("submit_hand_select"),
                "combat_selection_family"
            )
            | (
                "combat",
                Some("submit_grid_select"),
                "combat_selection_family"
            )
            | (
                "combat",
                Some("play_card" | "end_turn" | "use_potion"),
                "combat_atomic",
            )
    )
}

fn current_run_profile_supports_selected(
    site: &str,
    action_kind: Option<&str>,
    domain: &str,
    selected_candidate: &Value,
) -> bool {
    current_run_profile_supports(site, action_kind, domain)
        || (site == "combat"
            && action_kind == Some("submit_indexed_choice")
            && domain == "combat_indexed_choice"
            && retained_combat_card_reward_source_kind(selected_candidate).is_some())
}

fn current_run_profile_supports_step(step: &MirrorStep) -> bool {
    current_run_profile_supports_selected(
        &step.site,
        step.action_kind.as_deref(),
        &step.candidate_domain,
        &step.selected_candidate,
    ) || is_supported_event_deck_run_selection(step)
}

fn is_supported_event_deck_run_selection(step: &MirrorStep) -> bool {
    step.site == "run_choice"
        && step.action_kind.as_deref() == Some("submit_run_selection")
        && step.candidate_domain == "strategic"
        && step.run_selection.as_ref().is_some_and(|selection| {
            !selection.source_event.is_empty()
                && selection.scope == "Deck"
                && event_deck_grid_selection_reason(step).is_ok()
        })
}

fn retained_combat_card_reward_source_kind(selected_candidate: &Value) -> Option<&'static str> {
    let is_exact_card_choice = selected_candidate["action"]["kind"].as_str()
        == Some("submit_indexed_choice")
        && selected_candidate["action"]["choice_index"]
            .as_u64()
            .is_some()
        && selected_candidate["input_encoding"].as_str() == Some("submit_discover_choice_index")
        && selected_candidate["candidate"]["kind"].as_str() == Some("card")
        && selected_candidate["candidate"]["card_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
        && selected_candidate["candidate"]["upgrades"]
            .as_u64()
            .is_some();
    if !is_exact_card_choice {
        return None;
    }
    match selected_candidate["reason"]["kind"].as_str() {
        Some("discovery") => Some("combat_discovery"),
        Some("card_reward")
            if selected_candidate["reason"]["destination"].as_str() == Some("Hand") =>
        {
            Some("combat_discovery")
        }
        _ => None,
    }
}

fn retained_action_kind(value: &Value) -> Option<String> {
    optional_string(&value["selected_candidate"]["action"]["kind"]).or_else(|| {
        if value["selected_candidate"]["domain"].as_str() != Some("combat_selection_family") {
            return None;
        }
        match value["selected_candidate"]["family"]["input_encoding"].as_str() {
            Some("submit_selection_hand_card_uuids") => Some("submit_hand_select".to_owned()),
            Some("submit_selection_grid_card_uuids") => Some("submit_grid_select".to_owned()),
            _ => None,
        }
    })
}

fn required_string(value: &Value, field: &str) -> Result<String, String> {
    value[field]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("mirror source omitted string field {field}"))
}

fn optional_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn required_u64(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_u64()
        .ok_or_else(|| format!("mirror source omitted unsigned field {field}"))
}

fn required_usize(value: &Value, field: &str) -> Result<usize, String> {
    required_u64(value, field).and_then(|value| {
        usize::try_from(value).map_err(|_| format!("mirror source field {field} exceeds usize"))
    })
}

fn required_i64(value: &Value, field: &str) -> Result<i64, String> {
    value[field]
        .as_i64()
        .ok_or_else(|| format!("mirror source omitted integer field {field}"))
}

fn absolute_string(path: &Path) -> Result<String, String> {
    path.canonicalize()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| format!("failed to canonicalize {}: {error}", path.display()))
}

pub fn run_mirror_plan(
    output_dir: &Path,
    plan_path: &Path,
    expected_launch_plan: &Path,
    report_path: &Path,
    timeout: Duration,
) -> Result<MirrorExecutionReport, String> {
    let plan = read_mirror_plan(plan_path)?;
    validate_current_mirror_plan(&plan)?;
    let initial = status(output_dir)?;
    validate_mirror_initial_surface(&initial, expected_launch_plan)?;
    let created_at = unix_ms()?;
    let mut report = MirrorExecutionReport {
        schema: "sts-java-parity-mirror-execution-v4".to_owned(),
        provenance: "live_unclassified".to_owned(),
        run_play_evidence_identity: plan.source.run_play_evidence_identity.clone(),
        created_at_unix_ms: created_at,
        completed_at_unix_ms: created_at,
        passed: false,
        failure: None,
        plan_path: absolute_string(plan_path)?,
        plan_sha256: sha256_file(plan_path)?,
        expected_launch_plan: absolute_string(expected_launch_plan)?,
        session: initial.manifest.clone(),
        initial_frame: file_identity(Path::new(&initial.frame_path))?,
        resume: None,
        planned_steps: plan.steps.len(),
        completed_plan_steps: 0,
        steps: Vec::new(),
        successor_scalars: None,
        first_mismatch: None,
        evidence_scope: MirrorExecutionEvidenceScope {
            original_game_prefix_captured: false,
            planned_prefix_completed: false,
            expected_successor_reached: false,
            first_combat_victory_reached: false,
            first_reward_completed: false,
            post_reward_map_reached: false,
            scalar_pilot_compared: false,
            native_checkpoint_restored: false,
            checkpoint_suffix_captured: false,
            continuous_from_neow: true,
            successor_combat_projection_compared: false,
            complete_rust_semantic_fingerprint_compared: false,
            simulator_comparison: false,
            strategy_evidence: false,
            training_eligible: false,
        },
    };

    let outcome = execute_mirror_prefix(output_dir, &plan, timeout, &mut report);
    report.completed_at_unix_ms = unix_ms()?;
    match outcome {
        Ok(()) => {
            report.passed = true;
            report.evidence_scope.original_game_prefix_captured = true;
            report.evidence_scope.planned_prefix_completed = true;
            report.evidence_scope.expected_successor_reached = true;
            report.evidence_scope.first_combat_victory_reached = true;
            report.evidence_scope.first_reward_completed = true;
            report.evidence_scope.post_reward_map_reached = true;
            report.evidence_scope.scalar_pilot_compared = true;
        }
        Err(error) => report.failure = Some(error),
    }
    publish_json_create_new(report_path, &report)?;
    if report.passed {
        Ok(report)
    } else {
        Err(format!(
            "Java mirror prefix stopped at its first mismatch; durable report={}",
            absolute_string(report_path)?
        ))
    }
}

pub fn run_mirror_suffix(
    output_dir: &Path,
    plan_path: &Path,
    checkpoint_manifest_path: &Path,
    expected_launch_plan: &Path,
    report_path: &Path,
    timeout: Duration,
) -> Result<MirrorExecutionReport, String> {
    let plan = read_mirror_plan(plan_path)?;
    validate_current_mirror_plan(&plan)?;
    let checkpoint = read_native_combat_checkpoint(checkpoint_manifest_path, &plan)?;
    let initial = status(output_dir)?;
    validate_mirror_initial_surface(&initial, expected_launch_plan)?;
    let continue_actions = initial
        .actions
        .iter()
        .filter(|action| action.kind == "continue_run" && action.action_id == "continue_run")
        .count();
    if continue_actions != 1
        || initial.actions.len() != 1
        || !initial.actions[0].parameters_required
    {
        return Err(
            "checkpoint suffix requires one unique content-profile-bound continue_run main-menu action"
                .to_owned(),
        );
    }
    let created_at = unix_ms()?;
    let suffix_steps = &plan.steps[checkpoint.source_depth..];
    let mut report = MirrorExecutionReport {
        schema: "sts-java-parity-mirror-suffix-execution-v1".to_owned(),
        provenance: "natural_checkpoint_suffix".to_owned(),
        run_play_evidence_identity: plan.source.run_play_evidence_identity.clone(),
        created_at_unix_ms: created_at,
        completed_at_unix_ms: created_at,
        passed: false,
        failure: None,
        plan_path: absolute_string(plan_path)?,
        plan_sha256: sha256_file(plan_path)?,
        expected_launch_plan: absolute_string(expected_launch_plan)?,
        session: initial.manifest.clone(),
        initial_frame: file_identity(Path::new(&initial.frame_path))?,
        resume: Some(MirrorResumeEvidence {
            checkpoint_manifest_path: absolute_string(checkpoint_manifest_path)?,
            checkpoint_manifest_sha256: sha256_file(checkpoint_manifest_path)?,
            source_depth: checkpoint.source_depth,
            continuous_from_neow: false,
        }),
        planned_steps: suffix_steps.len(),
        completed_plan_steps: 0,
        steps: Vec::new(),
        successor_scalars: None,
        first_mismatch: None,
        evidence_scope: MirrorExecutionEvidenceScope {
            original_game_prefix_captured: false,
            planned_prefix_completed: false,
            expected_successor_reached: false,
            first_combat_victory_reached: false,
            first_reward_completed: false,
            post_reward_map_reached: false,
            scalar_pilot_compared: false,
            native_checkpoint_restored: false,
            checkpoint_suffix_captured: false,
            continuous_from_neow: false,
            successor_combat_projection_compared: false,
            complete_rust_semantic_fingerprint_compared: false,
            simulator_comparison: false,
            strategy_evidence: false,
            training_eligible: false,
        },
    };

    let outcome = (|| {
        let receipt = continue_run_with_content_profile(
            output_dir,
            &plan.run_start.content_profile,
            timeout,
        )?;
        if receipt.parameters != json!({"content_profile": plan.run_start.content_profile.as_str()})
        {
            return Err("checkpoint resume receipt lost its bound content profile".to_owned());
        }
        record_execution_step(&mut report, None, "continue_run", None, &receipt)?;
        let resumed = ready_mirror_status(output_dir, timeout, &mut report, "combat", None, None)?;
        if let Err(detail) = validate_resumed_checkpoint_root(&checkpoint, &resumed.frame) {
            return stop_with_mismatch(
                &mut report,
                MirrorMismatch {
                    kind: "native_checkpoint_restore_mismatch".to_owned(),
                    source_depth: None,
                    semantic_action: Some("continue_run".to_owned()),
                    frame_id: frame_id(&resumed).ok(),
                    detail,
                },
            );
        }
        report.evidence_scope.native_checkpoint_restored = true;
        execute_mirror_steps(output_dir, &plan, suffix_steps, timeout, &mut report)
    })();
    report.completed_at_unix_ms = unix_ms()?;
    match outcome {
        Ok(()) => {
            report.passed = true;
            report.evidence_scope.expected_successor_reached = true;
            report.evidence_scope.checkpoint_suffix_captured = true;
            report.evidence_scope.scalar_pilot_compared = true;
        }
        Err(error) => report.failure = Some(error),
    }
    publish_json_create_new(report_path, &report)?;
    if report.passed {
        Ok(report)
    } else {
        Err(format!(
            "Java mirror suffix stopped at its first mismatch; durable report={}",
            absolute_string(report_path)?
        ))
    }
}

pub fn compile_mirror_suffix_batch(
    checkpoint_manifest_path: &Path,
    mirror_plan_paths: &[std::path::PathBuf],
    output_path: &Path,
) -> Result<MirrorSuffixBatchPlan, String> {
    validate_mirror_suffix_batch_member_count(mirror_plan_paths.len())?;
    let mut members = Vec::with_capacity(mirror_plan_paths.len());
    let mut source_depth = None;
    for (index, plan_path) in mirror_plan_paths.iter().enumerate() {
        let plan = read_mirror_plan(plan_path)?;
        validate_current_mirror_plan(&plan)?;
        let checkpoint = read_native_combat_checkpoint(checkpoint_manifest_path, &plan)?;
        let suffix_steps = plan
            .steps
            .len()
            .checked_sub(checkpoint.source_depth)
            .filter(|count| *count > 0)
            .ok_or_else(|| {
                format!(
                    "batch member {index} does not extend the checkpoint with a non-empty suffix"
                )
            })?;
        if plan.scope.expected_successor_site != "combat"
            || effective_combat_boundary(&plan.scope) != COMBAT_BOUNDARY_PLAYER_DECISION
        {
            return Err(format!(
                "batch member {index} must end at a combat player decision"
            ));
        }
        if source_depth
            .replace(checkpoint.source_depth)
            .is_some_and(|previous| previous != checkpoint.source_depth)
        {
            return Err("batch members disagree on checkpoint source depth".to_owned());
        }
        members.push(MirrorSuffixBatchMember {
            member_id: format!("branch-{index:03}"),
            mirror_plan: file_identity(plan_path)?,
            suffix_steps,
        });
    }
    let batch = MirrorSuffixBatchPlan {
        schema: "sts-java-parity-mirror-suffix-batch-plan-v1".to_owned(),
        provenance: "natural_checkpoint_batch_plan".to_owned(),
        created_at_unix_ms: unix_ms()?,
        checkpoint_manifest: file_identity(checkpoint_manifest_path)?,
        source_depth: source_depth.ok_or_else(|| "batch omitted its source depth".to_owned())?,
        members,
        evidence_scope: MirrorSuffixBatchEvidenceScope {
            continuous_from_neow: false,
            simulator_comparison: false,
            strategy_evidence: false,
            training_eligible: false,
        },
    };
    publish_json_create_new(output_path, &batch)?;
    Ok(batch)
}

fn validate_mirror_suffix_batch_member_count(count: usize) -> Result<(), String> {
    if !(2..=64).contains(&count) {
        return Err("mirror suffix batch requires between 2 and 64 members".to_owned());
    }
    Ok(())
}

fn read_native_combat_checkpoint(
    checkpoint_manifest_path: &Path,
    target_plan: &MirrorPlan,
) -> Result<NativeCombatCheckpoint, String> {
    let bytes = fs::read(checkpoint_manifest_path).map_err(|error| {
        format!(
            "failed to read checkpoint manifest {}: {error}",
            checkpoint_manifest_path.display()
        )
    })?;
    let checkpoint: NativeCombatCheckpoint = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse checkpoint manifest {}: {error}",
            checkpoint_manifest_path.display()
        )
    })?;
    let full_prefix_source = checkpoint.provenance == "natural_full_prefix_native_autosave";
    let checkpoint_suffix_source =
        checkpoint.provenance == "natural_checkpoint_suffix_native_autosave";
    if checkpoint.schema != "sts-java-parity-native-combat-checkpoint-v1"
        || (!full_prefix_source && !checkpoint_suffix_source)
        || checkpoint.native_scope != "enter_room_combat"
        || checkpoint.storage_edited
        || checkpoint.evidence_scope["original_game_native_autosave"].as_bool() != Some(true)
        || checkpoint.evidence_scope["simulator_comparison"].as_bool() != Some(false)
        || checkpoint.evidence_scope["strategy_evidence"].as_bool() != Some(false)
        || checkpoint.evidence_scope["training_eligible"].as_bool() != Some(false)
        || (full_prefix_source
            && (checkpoint.parent_checkpoint.is_some()
                || checkpoint.evidence_scope["continuous_from_neow"].as_bool() != Some(true)))
        || (checkpoint_suffix_source
            && (checkpoint.parent_checkpoint.is_none()
                || checkpoint.evidence_scope["continuous_from_neow"].as_bool() != Some(false)
                || checkpoint.evidence_scope["native_checkpoint_restored"].as_bool() != Some(true)
                || checkpoint.evidence_scope["checkpoint_suffix_captured"].as_bool() != Some(true)))
    {
        return Err("checkpoint does not satisfy the natural combat restore contract".to_owned());
    }
    for identity in [
        &checkpoint.source_plan,
        &checkpoint.source_execution,
        &checkpoint.source_lifecycle,
        &checkpoint.root_frame,
        &checkpoint.autosave,
    ] {
        validate_native_checkpoint_file(identity)?;
    }
    if let Some(parent) = checkpoint.parent_checkpoint.as_ref() {
        validate_native_checkpoint_file(parent)?;
    }
    let source_plan = read_mirror_plan(Path::new(&checkpoint.source_plan.path))?;
    if checkpoint_suffix_source
        && source_plan
            .steps
            .last()
            .and_then(|step| step.action_kind.as_deref())
            != Some("choose_route_node")
    {
        return Err(
            "chained native checkpoint was not captured directly after a typed map route entered a new combat room"
                .to_owned(),
        );
    }
    let source_steps = source_plan
        .steps
        .iter()
        .map(checkpoint_prefix_step_projection)
        .collect::<Vec<_>>();
    let target_prefix_steps = target_plan
        .steps
        .get(..checkpoint.source_depth)
        .ok_or_else(|| "target mirror plan is shorter than the checkpoint prefix".to_owned())?;
    let target_prefix = target_prefix_steps
        .iter()
        .map(checkpoint_prefix_step_projection)
        .collect::<Vec<_>>();
    if checkpoint.source_depth == 0
        || checkpoint.source_depth != source_plan.steps.len()
        || checkpoint.source_depth > target_plan.steps.len()
        || source_plan.scope.expected_successor_site != "combat"
        || source_plan.run_start.seed != target_plan.run_start.seed
        || source_plan.run_start.character != target_plan.run_start.character
        || source_plan.run_start.ascension != target_plan.run_start.ascension
        || source_plan.run_start.content_profile != target_plan.run_start.content_profile
        || source_steps != target_prefix
    {
        return Err(
            "target mirror plan does not extend the checkpoint's exact natural prefix".to_owned(),
        );
    }
    let execution: MirrorExecutionReport = serde_json::from_slice(
        &fs::read(&checkpoint.source_execution.path)
            .map_err(|error| format!("failed to read checkpoint execution: {error}"))?,
    )
    .map_err(|error| format!("failed to parse checkpoint execution: {error}"))?;
    if !execution.passed || execution.plan_sha256 != checkpoint.source_plan.sha256 {
        return Err(
            "checkpoint source execution is incomplete or belongs to another plan".to_owned(),
        );
    }
    let expected_lifecycle_kind = if full_prefix_source {
        if execution.resume.is_some()
            || execution.planned_steps != checkpoint.source_depth
            || execution.completed_plan_steps != checkpoint.source_depth
            || !execution.evidence_scope.continuous_from_neow
            || !execution.evidence_scope.original_game_prefix_captured
        {
            return Err(
                "checkpoint source is not a completed continuous full-prefix execution".to_owned(),
            );
        }
        "natural_mirror_checkpoint"
    } else {
        let parent_identity = checkpoint.parent_checkpoint.as_ref().unwrap();
        let parent = read_native_combat_checkpoint(Path::new(&parent_identity.path), &source_plan)?;
        let resume = execution.resume.as_ref().ok_or_else(|| {
            "chained checkpoint source execution omitted its parent resume evidence".to_owned()
        })?;
        let suffix_steps = checkpoint
            .source_depth
            .checked_sub(parent.source_depth)
            .filter(|steps| *steps > 0)
            .ok_or_else(|| {
                "chained checkpoint did not advance beyond its parent depth".to_owned()
            })?;
        if execution.planned_steps != suffix_steps
            || execution.completed_plan_steps != suffix_steps
            || !execution.evidence_scope.native_checkpoint_restored
            || !execution.evidence_scope.checkpoint_suffix_captured
            || execution.evidence_scope.continuous_from_neow
            || resume.source_depth != parent.source_depth
            || resume.checkpoint_manifest_sha256 != parent_identity.sha256
            || !same_file_path(
                Path::new(&resume.checkpoint_manifest_path),
                Path::new(&parent_identity.path),
            )?
        {
            return Err(
                "checkpoint source is not a completed exact parent-checkpoint suffix".to_owned(),
            );
        }
        "natural_mirror_checkpoint_suffix"
    };
    let lifecycle: Value = serde_json::from_slice(
        &fs::read(&checkpoint.source_lifecycle.path)
            .map_err(|error| format!("failed to read checkpoint lifecycle: {error}"))?,
    )
    .map_err(|error| format!("failed to parse checkpoint lifecycle: {error}"))?;
    if lifecycle["passed"].as_bool() != Some(true)
        || lifecycle["case_kind"].as_str() != Some(expected_lifecycle_kind)
        || lifecycle["installed_storage_guard"]["unchanged"].as_bool() != Some(true)
    {
        return Err("checkpoint lifecycle did not pass its isolated storage guard".to_owned());
    }
    let expected_combat_boundary = effective_combat_boundary(&source_plan.scope);
    let recorded_combat_boundary = checkpoint.root_decision["boundary"]
        .as_str()
        .unwrap_or(COMBAT_BOUNDARY_PLAYER_DECISION);
    if checkpoint.root_decision["kind"].as_str() != Some("combat")
        || recorded_combat_boundary != expected_combat_boundary
        || checkpoint.autosave_summary["seed"].as_str() != Some(target_plan.run_start.seed.as_str())
        || checkpoint.storage["fingerprint_sha256"].as_str().is_none()
    {
        return Err("checkpoint root metadata is incomplete or belongs to another run".to_owned());
    }
    let root_frame: Value = serde_json::from_slice(
        &fs::read(&checkpoint.root_frame.path)
            .map_err(|error| format!("failed to read checkpoint root frame: {error}"))?,
    )
    .map_err(|error| format!("failed to parse checkpoint root frame: {error}"))?;
    if let Some(detail) = mirror_run_contract_error(&target_plan.run_start.objective, &root_frame) {
        return Err(format!(
            "checkpoint violates the target run objective: {detail}"
        ));
    }
    if !java_frame_matches_combat_boundary(&root_frame, expected_combat_boundary)
        || !source_plan
            .scope
            .expected_successor_monster_ids
            .as_deref()
            .is_some_and(|expected| {
                java_combat_monster_ids(&root_frame)
                    .is_ok_and(|actual| ordered_enemy_ids_match(expected, &actual))
            })
    {
        return Err(
            "checkpoint root frame is not the target plan's exact combat successor".to_owned(),
        );
    }
    Ok(checkpoint)
}

/// Imports one natural native combat checkpoint through the mirror owner's
/// full validation contract. This function performs no restore and mutates no
/// Java or Rust state.
pub fn validate_native_combat_checkpoint_v1(
    checkpoint_manifest_path: &Path,
    target_plan_path: &Path,
) -> Result<ValidatedNativeCombatCheckpointV1, String> {
    let target_plan = read_mirror_plan(target_plan_path)?;
    validate_current_mirror_plan(&target_plan)?;
    let checkpoint = read_native_combat_checkpoint(checkpoint_manifest_path, &target_plan)?;
    let captured_root_frame = serde_json::from_slice(
        &fs::read(&checkpoint.root_frame.path)
            .map_err(|error| format!("failed to read checkpoint root frame: {error}"))?,
    )
    .map_err(|error| format!("failed to parse checkpoint root frame: {error}"))?;
    let manifest = file_identity(checkpoint_manifest_path)?;
    let checkpoint_directory = checkpoint_manifest_path
        .parent()
        .ok_or_else(|| "checkpoint manifest has no parent directory".to_owned())?
        .canonicalize()
        .map_err(|error| format!("failed to resolve checkpoint directory: {error}"))?
        .to_string_lossy()
        .into_owned();
    Ok(ValidatedNativeCombatCheckpointV1 {
        manifest,
        checkpoint_directory,
        provenance: checkpoint.provenance.clone(),
        source_depth: checkpoint.source_depth,
        content_profile: target_plan.run_start.content_profile.clone(),
        source_plan: public_checkpoint_file_identity(&checkpoint.source_plan),
        source_execution: public_checkpoint_file_identity(&checkpoint.source_execution),
        source_lifecycle: public_checkpoint_file_identity(&checkpoint.source_lifecycle),
        parent_checkpoint: checkpoint
            .parent_checkpoint
            .as_ref()
            .map(public_checkpoint_file_identity),
        root_frame: public_checkpoint_file_identity(&checkpoint.root_frame),
        autosave: public_checkpoint_file_identity(&checkpoint.autosave),
        captured_root_frame,
    })
}

/// Verifies that a live frame is the semantic native restore of an already
/// validated checkpoint. Runtime UUIDs are intentionally not treated as
/// persistent save identity; paired-root card binding must be rebuilt from the
/// fresh frame's typed source evidence.
pub fn validate_resumed_native_checkpoint_root_v1(
    checkpoint_manifest_path: &Path,
    target_plan_path: &Path,
    resumed_frame: &Value,
) -> Result<ValidatedNativeCombatCheckpointV1, String> {
    let target_plan = read_mirror_plan(target_plan_path)?;
    validate_current_mirror_plan(&target_plan)?;
    let checkpoint = read_native_combat_checkpoint(checkpoint_manifest_path, &target_plan)?;
    validate_resumed_checkpoint_root(&checkpoint, resumed_frame)?;
    validate_native_combat_checkpoint_v1(checkpoint_manifest_path, target_plan_path)
}

fn public_checkpoint_file_identity(identity: &NativeCheckpointFileIdentity) -> MirrorFileIdentity {
    MirrorFileIdentity {
        path: identity.path.clone(),
        bytes: identity.length,
        sha256: identity.sha256.clone(),
    }
}

fn validate_resumed_checkpoint_root(
    checkpoint: &NativeCombatCheckpoint,
    resumed_frame: &Value,
) -> Result<(), String> {
    let captured_bytes = fs::read(&checkpoint.root_frame.path)
        .map_err(|error| format!("failed to read captured checkpoint root: {error}"))?;
    let captured_frame: Value = serde_json::from_slice(&captured_bytes)
        .map_err(|error| format!("failed to parse captured checkpoint root: {error}"))?;
    let captured_projection = checkpoint_semantic_projection(&captured_frame);
    let resumed_projection = checkpoint_semantic_projection(resumed_frame);
    if captured_projection
        != project_candidate_to_captured_contract(&captured_projection, &resumed_projection)
    {
        return Err(
            "original-game Continue did not reconstruct the captured combat root's stable semantic projection"
                .to_owned(),
        );
    }
    Ok(())
}

fn checkpoint_semantic_projection(frame: &Value) -> Value {
    let mut rng_internal = frame["rng_internal"].clone();
    let has_emerald_key = frame["public_base"]["run"]["has_emerald_key"]
        .as_bool()
        .unwrap_or(false);
    if let Some(streams) = rng_internal["run_streams"].as_array_mut() {
        streams.retain(|stream| {
            let owner = stream["owner"].as_str();
            owner != Some("com.megacrit.cardcrawl.neow.NeowEvent.rng")
                && !(has_emerald_key
                    && owner == Some("com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng"))
        });
    }
    strip_runtime_identity(&json!({
        "public_base": frame["public_base"],
        "parity_internal": frame["parity_internal"],
        "rng_internal": rng_internal,
    }))
}

fn strip_runtime_identity(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(key, _)| {
                    *key != "instance_uuid" && *key != "instance_id" && *key != "card_uuid"
                })
                .map(|(key, value)| (key.clone(), strip_runtime_identity(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(strip_runtime_identity).collect()),
        other => other.clone(),
    }
}

/// A native checkpoint can only prove the fields that its captured frame
/// actually recorded. Later Mod versions may add parity-only observations;
/// those fields belong to the caller's current preflight and must not make an
/// otherwise identical original-game restore fail. Fields and array members
/// present in the captured contract remain exact, including their absence from
/// the resumed frame.
fn project_candidate_to_captured_contract(captured: &Value, candidate: &Value) -> Value {
    match (captured, candidate) {
        (Value::Object(captured), Value::Object(candidate)) => Value::Object(
            captured
                .iter()
                .map(|(key, captured_value)| {
                    let projected = candidate.get(key).map_or(Value::Null, |candidate_value| {
                        project_candidate_to_captured_contract(captured_value, candidate_value)
                    });
                    (key.clone(), projected)
                })
                .collect(),
        ),
        (Value::Array(captured), Value::Array(candidate)) if captured.len() == candidate.len() => {
            Value::Array(
                captured
                    .iter()
                    .zip(candidate)
                    .map(|(captured, candidate)| {
                        project_candidate_to_captured_contract(captured, candidate)
                    })
                    .collect(),
            )
        }
        (_, candidate) => candidate.clone(),
    }
}

fn validate_native_checkpoint_file(identity: &NativeCheckpointFileIdentity) -> Result<(), String> {
    let path = Path::new(&identity.path);
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "checkpoint-bound file is unavailable {}: {error}",
            path.display()
        )
    })?;
    if metadata.len() != identity.length || sha256_file(path)? != identity.sha256 {
        return Err(format!("checkpoint-bound file changed: {}", path.display()));
    }
    Ok(())
}

fn same_file_path(left: &Path, right: &Path) -> Result<bool, String> {
    let left = left
        .canonicalize()
        .map_err(|error| format!("failed to resolve {}: {error}", left.display()))?;
    let right = right
        .canonicalize()
        .map_err(|error| format!("failed to resolve {}: {error}", right.display()))?;
    Ok(left == right)
}

fn execute_mirror_prefix(
    output_dir: &Path,
    plan: &MirrorPlan,
    timeout: Duration,
    report: &mut MirrorExecutionReport,
) -> Result<(), String> {
    let java_seed_token = java_seed_token_for_engine_seed(&plan.run_start.seed)?;
    let receipt = start_run_with_content_profile(
        output_dir,
        &plan.run_start.character,
        plan.run_start.ascension,
        Some(&java_seed_token),
        &plan.run_start.content_profile,
        timeout,
    )?;
    record_execution_step(report, None, "start_run", None, &receipt)?;

    execute_mirror_steps(output_dir, plan, &plan.steps, timeout, report)
}

/// Match `SeedHelper.getString(long)` for an unsigned Java `long` bit pattern.
/// The original game accepts this base-35 token and reconstructs the exact
/// `Settings.seed` with `SeedHelper.getLong`. Seed zero is represented as `0`
/// because the original helper returns an empty display string for zero, while
/// the controlled `start_run` API deliberately rejects an empty seed.
pub fn java_seed_token_for_engine_seed(seed: &str) -> Result<String, String> {
    const ALPHABET: &[u8; 35] = b"0123456789ABCDEFGHIJKLMNPQRSTUVWXYZ";

    let mut value = seed.parse::<u64>().map_err(|error| {
        format!("mirror run seed must be a decimal unsigned 64-bit engine seed: {error}")
    })?;
    if value == 0 {
        return Ok("0".to_owned());
    }

    let mut reversed = Vec::new();
    while value != 0 {
        reversed.push(ALPHABET[(value % 35) as usize]);
        value /= 35;
    }
    reversed.reverse();
    String::from_utf8(reversed)
        .map_err(|error| format!("internal Java seed encoding produced invalid UTF-8: {error}"))
}

fn execute_mirror_steps(
    output_dir: &Path,
    plan: &MirrorPlan,
    steps: &[MirrorStep],
    timeout: Duration,
    report: &mut MirrorExecutionReport,
) -> Result<(), String> {
    let mut pending_postcondition = None::<&MirrorStep>;
    for step in steps {
        let view = ready_mirror_status(
            output_dir,
            timeout,
            report,
            &step.site,
            step.action_kind.as_deref(),
            step.selected_candidate["action"]["screen"].as_i64(),
        )?;
        if let Some(detail) = mirror_run_contract_error(&plan.run_start.objective, &view.frame) {
            return stop_with_mismatch(
                report,
                MirrorMismatch {
                    kind: "run_objective_mismatch".to_owned(),
                    source_depth: Some(step.depth),
                    semantic_action: Some(step.semantic_action.clone()),
                    frame_id: frame_id(&view).ok(),
                    detail,
                },
            );
        }
        if let Some(previous_step) = pending_postcondition.take() {
            verify_mirror_postcondition(report, &view, previous_step)?;
        }
        if !java_decision_matches_rust_step(&view, step) {
            return stop_with_mismatch(
                report,
                MirrorMismatch {
                    kind: "typed_decision_site_mismatch".to_owned(),
                    source_depth: Some(step.depth),
                    semantic_action: Some(step.semantic_action.clone()),
                    frame_id: frame_id(&view).ok(),
                    detail: format!(
                        "Java decision kind {:?} does not match retained Rust site {:?}",
                        view.frame["public_base"]["decision"]["kind"].as_str(),
                        step.site
                    ),
                },
            );
        }
        let comparison = compare_scalars(&view, &step.before);
        if scalar_comparison_has_mismatch(&comparison) {
            return stop_with_mismatch(
                report,
                MirrorMismatch {
                    kind: "source_scalar_mismatch".to_owned(),
                    source_depth: Some(step.depth),
                    semantic_action: Some(step.semantic_action.clone()),
                    frame_id: frame_id(&view).ok(),
                    detail: format!(
                        "Java before-state disagrees with retained Rust scalars: {comparison:?}"
                    ),
                },
            );
        }
        if let Some(expected_combat) = step.combat_before.as_ref() {
            if let Some(detail) = compare_combat_projection(&view.frame, expected_combat) {
                return stop_with_mismatch(
                    report,
                    MirrorMismatch {
                        kind: "source_combat_projection_mismatch".to_owned(),
                        source_depth: Some(step.depth),
                        semantic_action: Some(step.semantic_action.clone()),
                        frame_id: frame_id(&view).ok(),
                        detail,
                    },
                );
            }
        }
        if step.action_kind.as_deref() == Some("remove_card") {
            if let Err(detail) =
                execute_shop_remove_step(output_dir, &view, step, comparison, timeout, report)
            {
                return stop_with_mismatch(
                    report,
                    MirrorMismatch {
                        kind: "typed_action_resolution_failed".to_owned(),
                        source_depth: Some(step.depth),
                        semantic_action: Some(step.semantic_action.clone()),
                        frame_id: frame_id(&view).ok(),
                        detail,
                    },
                );
            }
            report.completed_plan_steps += 1;
            continue;
        }
        let action = match resolve_mirror_action(&view, step) {
            Ok(action) => action,
            Err(detail) => {
                return stop_with_mismatch(
                    report,
                    MirrorMismatch {
                        kind: "typed_action_resolution_failed".to_owned(),
                        source_depth: Some(step.depth),
                        semantic_action: Some(step.semantic_action.clone()),
                        frame_id: frame_id(&view).ok(),
                        detail,
                    },
                )
            }
        };
        let receipt = if action.kind == "submit_hand_select" {
            let selected_uuids = selected_hand_card_uuids(&view, step)?;
            submit_hand_select(output_dir, &selected_uuids, timeout)?
        } else if action.kind == "submit_grid_select" {
            let selected_uuid = selected_mirror_grid_card_uuid(&view, step)?;
            submit_grid_select(output_dir, &[selected_uuid], timeout)?
        } else {
            act(output_dir, &action.action_id, timeout)?
        };
        record_execution_step(
            report,
            Some(step.depth),
            &step.semantic_action,
            Some(comparison),
            &receipt,
        )?;
        report.completed_plan_steps += 1;
        if matches!(
            step.action_kind.as_deref(),
            Some("smith" | "discard_run_potion")
        ) {
            pending_postcondition = Some(step);
        }
    }

    let successor = ready_mirror_status(
        output_dir,
        timeout,
        report,
        &plan.scope.expected_successor_site,
        None,
        None,
    )?;
    if let Some(previous_step) = pending_postcondition.take() {
        verify_mirror_postcondition(report, &successor, previous_step)?;
    }
    if !java_successor_matches_scope(&successor, &plan.scope) {
        return stop_with_mismatch(
            report,
            MirrorMismatch {
                kind: "unexpected_planned_prefix_successor".to_owned(),
                source_depth: None,
                semantic_action: None,
                frame_id: frame_id(&successor).ok(),
                detail: format!(
                    "The planned prefix did not settle at the expected typed {:?} decision boundary={:?}; Java boundary={:?} decision={:?}",
                    plan.scope.expected_successor_site,
                    plan.scope.expected_successor_combat_boundary,
                    successor.frame["boundary"]["kind"].as_str(),
                    successor.frame["public_base"]["decision"]["kind"].as_str()
                ),
            },
        );
    }
    if let Some(expected) = plan.scope.expected_successor_monster_ids.as_deref() {
        let actual = java_combat_monster_ids(&successor.frame)?;
        if !ordered_enemy_ids_match(expected, &actual) {
            return stop_with_mismatch(
                report,
                MirrorMismatch {
                    kind: "planned_prefix_successor_monsters_mismatch".to_owned(),
                    source_depth: None,
                    semantic_action: None,
                    frame_id: frame_id(&successor).ok(),
                    detail: format!(
                        "The planned prefix reached the expected combat site but Java monsters {actual:?} do not match retained Rust monsters {expected:?}"
                    ),
                },
            );
        }
    }
    if let Some(expected) = plan.scope.expected_successor_scalars.as_ref() {
        let comparison = compare_scalars(&successor, expected);
        report.successor_scalars = Some(comparison.clone());
        if scalar_comparison_has_mismatch(&comparison) {
            return stop_with_mismatch(
                report,
                MirrorMismatch {
                    kind: "successor_scalar_mismatch".to_owned(),
                    source_depth: None,
                    semantic_action: None,
                    frame_id: frame_id(&successor).ok(),
                    detail: format!(
                        "Java successor disagrees with retained Rust scalars: {comparison:?}"
                    ),
                },
            );
        }
    }
    if let Some(expected) = plan.scope.expected_successor_combat.as_ref() {
        if let Some(detail) = compare_combat_projection(&successor.frame, expected) {
            return stop_with_mismatch(
                report,
                MirrorMismatch {
                    kind: "successor_combat_projection_mismatch".to_owned(),
                    source_depth: None,
                    semantic_action: None,
                    frame_id: frame_id(&successor).ok(),
                    detail,
                },
            );
        }
        report.evidence_scope.successor_combat_projection_compared = true;
    }
    Ok(())
}

fn execute_shop_remove_step(
    output_dir: &Path,
    shop_view: &LiveStatus,
    step: &MirrorStep,
    comparison: MirrorScalarComparison,
    timeout: Duration,
    report: &mut MirrorExecutionReport,
) -> Result<(), String> {
    let action = &step.selected_candidate["action"];
    let card_ordinal = action["card_ordinal"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust remove-card action omitted card_ordinal".to_owned())?;
    let card_id = action["card"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust remove-card action omitted card ID".to_owned())?;
    let upgrades = action["upgrades"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust remove-card action omitted upgrades".to_owned())?;
    let price = action["price"]
        .as_i64()
        .filter(|value| *value >= 0)
        .ok_or_else(|| "Rust remove-card action omitted non-negative price".to_owned())?;

    let begin = shop_view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "begin_shop_purge" && candidate.shop_price == Some(price)
        })
        .collect::<Vec<_>>();
    let [begin] = begin.as_slice() else {
        return Err("Java shop did not expose one unique typed purge entry".to_owned());
    };
    let receipt = act(output_dir, &begin.action_id, timeout)?;
    record_execution_step(report, None, "java_begin_shop_purge", None, &receipt)?;

    let card_view = status(output_dir)?;
    if card_view.frame["public_base"]["decision"]["kind"].as_str() != Some("shop_purge_card") {
        return Err("Java shop purge did not settle at typed card selection".to_owned());
    }
    let choices = card_view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "choose_shop_purge_card"
                && candidate.card_ordinal == Some(card_ordinal)
                && candidate.card_upgrades == Some(upgrades)
                && candidate
                    .card_id
                    .as_deref()
                    .is_some_and(|java_id| game_ids_match(card_id, java_id))
        })
        .collect::<Vec<_>>();
    let [choice] = choices.as_slice() else {
        return Err(format!(
            "Java shop purge did not expose one unique card {card_id:?} upgrades={upgrades} ordinal={card_ordinal}"
        ));
    };
    let removed_uuid = choice
        .card_uuid
        .clone()
        .ok_or_else(|| "Java shop purge card omitted its UUID".to_owned())?;
    let receipt = act(output_dir, &choice.action_id, timeout)?;
    record_execution_step(
        report,
        Some(step.depth),
        &step.semantic_action,
        Some(comparison),
        &receipt,
    )?;

    let successor = status(output_dir)?;
    if successor.frame["public_base"]["decision"]["kind"].as_str() != Some("shop") {
        return Err("Java shop purge did not return to the typed shop decision".to_owned());
    }
    let expected_gold = step
        .before
        .gold
        .checked_sub(price)
        .ok_or_else(|| "shop purge expected gold underflowed".to_owned())?;
    if successor.frame["public_base"]["run"]["gold"].as_i64() != Some(expected_gold) {
        return Err(format!(
            "Java shop purge did not spend the exact price: before={} price={price}",
            step.before.gold
        ));
    }
    let master_deck = successor.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "Java shop purge successor omitted the typed master deck".to_owned())?;
    if master_deck
        .iter()
        .any(|card| card["card_uuid"].as_str() == Some(removed_uuid.as_str()))
    {
        return Err("Java shop purge successor still contains the selected card UUID".to_owned());
    }
    if successor
        .actions
        .iter()
        .any(|candidate| candidate.kind == "begin_shop_purge")
    {
        return Err("Java shop purge successor still exposes a second purge".to_owned());
    }
    Ok(())
}

fn current_combat_monster_ids(head: &Value) -> Result<Vec<String>, String> {
    let monsters = head["current_decision"]["observation"]["observation"]["monsters"]
        .as_array()
        .ok_or_else(|| "combat mirror source omitted its typed monster list".to_owned())?;
    let ids = monsters
        .iter()
        .map(|monster| {
            monster["enemy"]["enemy_id"]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "combat mirror source contains an unknown monster".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Err("combat mirror source has no monsters".to_owned());
    }
    Ok(ids)
}

fn current_combat_successor_projection(
    head: &Value,
) -> Result<(MirrorPublicScalars, MirrorCombatProjection), String> {
    let decision = &head["current_decision"];
    if decision["site"].as_str() != Some("combat")
        || decision["observation"]["domain"].as_str() != Some("combat")
    {
        return Err("combat mirror endpoint omitted its typed current combat decision".to_owned());
    }
    let run = &decision["observation"]["run_context"];
    let combat = &decision["observation"]["observation"];
    let player = &combat["player"];
    let turn = &combat["turn"];

    let scalars = MirrorPublicScalars {
        act: value_i64(&run["act"], "combat endpoint act")?,
        floor: value_i64(&run["floor"], "combat endpoint floor")?,
        current_hp: value_i64(&player["hp"], "combat endpoint current HP")?,
        max_hp: value_i64(&player["max_hp"], "combat endpoint max HP")?,
        gold: value_i64(&player["gold"], "combat endpoint gold")?,
    };

    let hand = combat["cards"]["hand"]["cards"]
        .as_array()
        .ok_or_else(|| "combat mirror endpoint omitted its public ordered hand".to_owned())?
        .iter()
        .enumerate()
        .map(|(index, card)| {
            Ok(MirrorCombatCard {
                card_id: required_string(card, "card_id").map_err(|error| {
                    format!("combat endpoint hand[{index}] has no card identity: {error}")
                })?,
                upgrades: value_u8(
                    &card["upgrades"],
                    &format!("combat endpoint hand[{index}] upgrades"),
                )?,
                cost_for_turn: value_i32(
                    &card["effective_cost"],
                    &format!("combat endpoint hand[{index}] effective cost"),
                )?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let potions = combat["potions"]
        .as_array()
        .ok_or_else(|| "combat mirror endpoint omitted its potion slots".to_owned())?
        .iter()
        .enumerate()
        .map(|(slot, potion)| {
            if potion.is_null() {
                Ok(None)
            } else {
                required_string(potion, "potion_id")
                    .map(Some)
                    .map_err(|error| {
                        format!("combat endpoint potion slot {slot} has no identity: {error}")
                    })
            }
        })
        .collect::<Result<Vec<_>, String>>()?;

    let player_powers = current_combat_powers(&player["powers"], "combat endpoint player")?;
    let source_monsters = combat["monsters"]
        .as_array()
        .ok_or_else(|| "combat mirror endpoint omitted its typed monster list".to_owned())?;
    let mut same_enemy_counts = BTreeMap::<String, usize>::new();
    let monsters = source_monsters
        .iter()
        .enumerate()
        .map(|(source_index, monster)| {
            let enemy_id = required_string(&monster["enemy"], "enemy_id").map_err(|error| {
                format!("combat endpoint monster[{source_index}] has no identity: {error}")
            })?;
            let same_enemy_ordinal = *same_enemy_counts.entry(enemy_id.clone()).or_default();
            *same_enemy_counts.get_mut(&enemy_id).unwrap() += 1;
            Ok(MirrorCombatMonster {
                source_index,
                source_slot: value_u8(
                    &monster["slot"],
                    &format!("combat endpoint monster[{source_index}] slot"),
                )?,
                enemy_id,
                same_enemy_ordinal,
                hp: value_i32(
                    &monster["hp"],
                    &format!("combat endpoint monster[{source_index}] HP"),
                )?,
                max_hp: value_i32(
                    &monster["max_hp"],
                    &format!("combat endpoint monster[{source_index}] max HP"),
                )?,
                block: value_i32(
                    &monster["block"],
                    &format!("combat endpoint monster[{source_index}] block"),
                )?,
                alive: monster["alive"].as_bool().ok_or_else(|| {
                    format!("combat endpoint monster[{source_index}] omitted alive")
                })?,
                intent: current_combat_intent_kind(&monster["intent"]["intent"], source_index)?,
                powers: current_combat_powers(
                    &monster["powers"],
                    &format!("combat endpoint monster[{source_index}]"),
                )?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if monsters.is_empty() {
        return Err("combat mirror endpoint has no monsters".to_owned());
    }

    Ok((
        scalars,
        MirrorCombatProjection {
            turn_index: value_u32(&turn["turn_count"], "combat endpoint turn")?,
            energy: value_u8(&turn["energy"], "combat endpoint energy")?,
            player_block: value_i32(&player["block"], "combat endpoint player block")?,
            hand,
            potions,
            player_powers,
            monsters,
        },
    ))
}

fn current_combat_powers(value: &Value, scope: &str) -> Result<Vec<MirrorCombatPower>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{scope} omitted its power list"))?
        .iter()
        .enumerate()
        .map(|(index, power)| {
            Ok(MirrorCombatPower {
                power_id: required_string(power, "power")
                    .map_err(|error| format!("{scope} power[{index}] has no identity: {error}"))?,
                amount: value_i32(&power["amount"], &format!("{scope} power[{index}] amount"))?,
            })
        })
        .collect()
}

fn current_combat_intent_kind(
    value: &Value,
    monster_index: usize,
) -> Result<Option<String>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let variant = value.as_str().or_else(|| {
        value
            .as_object()
            .filter(|fields| fields.len() == 1)
            .and_then(|fields| fields.keys().next().map(String::as_str))
    });
    let kind = match variant {
        Some("Attack") => "attack",
        Some("AttackBuff") => "attack_buff",
        Some("AttackSustain") => "attack_sustain",
        Some("AttackDebuff") => "attack_debuff",
        Some("AttackDefend") => "attack_defend",
        Some("Buff") => "buff",
        Some("Heal") => "heal",
        Some("Debuff") => "debuff",
        Some("StrongDebuff") => "strong_debuff",
        Some("Debug") => "debug",
        Some("Defend") => "defend",
        Some("DefendDebuff") => "defend_debuff",
        Some("DefendBuff") => "defend_buff",
        Some("Summon") => "summon",
        Some("Escape") => "escape",
        Some("Magic") => "magic",
        Some("None") => "none",
        Some("Sleep") => "sleep",
        Some("Stun") => "stun",
        Some("Unknown") => "unknown",
        _ => {
            return Err(format!(
                "combat endpoint monster[{monster_index}] has an unknown intent encoding {value}"
            ))
        }
    };
    Ok(Some(kind.to_owned()))
}

fn value_i64(value: &Value, label: &str) -> Result<i64, String> {
    value
        .as_i64()
        .ok_or_else(|| format!("{label} is not an integer"))
}

fn value_i32(value: &Value, label: &str) -> Result<i32, String> {
    value_i64(value, label)
        .and_then(|number| i32::try_from(number).map_err(|_| format!("{label} is outside i32")))
}

fn value_u32(value: &Value, label: &str) -> Result<u32, String> {
    value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| format!("{label} is outside u32"))
}

fn value_u8(value: &Value, label: &str) -> Result<u8, String> {
    value
        .as_u64()
        .and_then(|number| u8::try_from(number).ok())
        .ok_or_else(|| format!("{label} is outside u8"))
}

fn classify_expected_successor_combat_boundary(
    head: &Value,
    expected_successor_site: &str,
) -> Result<Option<String>, String> {
    if expected_successor_site != "combat" {
        return Ok(None);
    }
    if rust_head_is_initial_toolbox_overlay(head) {
        return Ok(Some(COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD.to_owned()));
    }

    let contains_indexed_card_choice = head["current_decision"]["candidates"]
        .as_array()
        .is_some_and(|candidates| {
            candidates.iter().any(|entry| {
                entry["candidate"]["domain"].as_str() == Some("combat_indexed_choice")
                    && entry["candidate"]["input_encoding"].as_str()
                        == Some("submit_discover_choice_index")
            })
        });
    if contains_indexed_card_choice {
        return Err(
            "combat mirror endpoint contains a card-choice overlay that is not the closed initial Toolbox contract"
                .to_owned(),
        );
    }
    Ok(Some(COMBAT_BOUNDARY_PLAYER_DECISION.to_owned()))
}

fn rust_head_is_initial_toolbox_overlay(head: &Value) -> bool {
    let decision = &head["current_decision"];
    let observation = &decision["observation"]["observation"];
    let candidates = match decision["candidates"].as_array() {
        Some(candidates) if candidates.len() == 3 => candidates,
        _ => return false,
    };
    if decision["site"].as_str() != Some("combat")
        || observation["turn"]["turn_count"].as_u64() != Some(0)
        || observation["turn"]["counters"]["cards_played_this_turn"].as_u64() != Some(0)
        || !observation["cards"]["hand"]["cards"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !observation["player"]["relics"]
            .as_array()
            .is_some_and(|relics| {
                relics
                    .iter()
                    .any(|relic| relic["id"].as_str() == Some("Toolbox"))
            })
    {
        return false;
    }

    let choice_indices = candidates
        .iter()
        .filter_map(|entry| {
            let candidate = &entry["candidate"];
            (candidate["domain"].as_str() == Some("combat_indexed_choice")
                && candidate["action"]["kind"].as_str() == Some("submit_indexed_choice")
                && candidate["input_encoding"].as_str() == Some("submit_discover_choice_index")
                && candidate["reason"]["kind"].as_str() == Some("card_reward")
                && candidate["reason"]["destination"].as_str() == Some("Hand")
                && candidate["candidate"]["kind"].as_str() == Some("card")
                && candidate["candidate"]["card_id"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty())
                && candidate["candidate"]["upgrades"].as_u64().is_some())
            .then(|| candidate["action"]["choice_index"].as_u64())
            .flatten()
        })
        .collect::<BTreeSet<_>>();
    choice_indices == BTreeSet::from([0, 1, 2])
}

fn java_combat_monster_ids(frame: &Value) -> Result<Vec<String>, String> {
    let monsters = frame["public_base"]["combat"]["monsters"]
        .as_array()
        .ok_or_else(|| "Java combat successor omitted its typed monster list".to_owned())?;
    let ids = monsters
        .iter()
        .map(|monster| {
            monster["id"]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "Java combat successor contains a monster without an ID".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Err("Java combat successor has no monsters".to_owned());
    }
    Ok(ids)
}

fn ordered_enemy_ids_match(expected: &[String], actual: &[String]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(left, right)| enemy_ids_match(left, right))
}

fn verify_mirror_postcondition(
    report: &mut MirrorExecutionReport,
    view: &LiveStatus,
    step: &MirrorStep,
) -> Result<(), String> {
    let result = match step.action_kind.as_deref() {
        Some("smith") => {
            verify_smith_postcondition(&view.frame, &step.selected_candidate["action"])
        }
        Some("discard_run_potion") => {
            verify_discard_potion_postcondition(view, &step.selected_candidate["action"])
        }
        _ => Ok(()),
    };
    match result {
        Ok(()) => Ok(()),
        Err(detail) => stop_with_mismatch(
            report,
            MirrorMismatch {
                kind: "typed_action_postcondition_mismatch".to_owned(),
                source_depth: Some(step.depth),
                semantic_action: Some(step.semantic_action.clone()),
                frame_id: frame_id(view).ok(),
                detail,
            },
        ),
    }
}

fn verify_smith_postcondition(frame: &Value, action: &Value) -> Result<(), String> {
    let card_ordinal = action["card_ordinal"]
        .as_u64()
        .ok_or_else(|| "Rust smith action omitted card_ordinal".to_owned())?;
    let card_id = action["card"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust smith action omitted card ID".to_owned())?;
    let upgrades_before = action["upgrades"]
        .as_u64()
        .ok_or_else(|| "Rust smith action omitted upgrades".to_owned())?;
    let master_deck = frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "Java smith successor omitted the typed master deck".to_owned())?;
    let card = master_deck
        .iter()
        .find(|candidate| candidate["ordinal"].as_u64() == Some(card_ordinal))
        .ok_or_else(|| {
            format!("Java smith successor omitted master-deck ordinal {card_ordinal}")
        })?;
    let java_card_id = card["card_id"]
        .as_str()
        .ok_or_else(|| "Java smith successor card omitted card_id".to_owned())?;
    if !game_ids_match(card_id, java_card_id) {
        return Err(format!(
            "Java smith successor changed master-deck ordinal {card_ordinal} from expected card {card_id:?} to {java_card_id:?}"
        ));
    }
    let upgrades_after = card["upgrades"]
        .as_u64()
        .ok_or_else(|| "Java smith successor card omitted upgrades".to_owned())?;
    if upgrades_after != upgrades_before + 1 {
        return Err(format!(
            "Java smith successor did not upgrade {card_id:?} at ordinal {card_ordinal}: before={upgrades_before} after={upgrades_after}"
        ));
    }
    Ok(())
}

fn verify_discard_potion_postcondition(view: &LiveStatus, action: &Value) -> Result<(), String> {
    let slot = action["slot"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust run-potion discard omitted slot".to_owned())?;
    if view
        .actions
        .iter()
        .any(|candidate| candidate.kind == "discard_potion" && candidate.potion_slot == Some(slot))
    {
        return Err(format!(
            "Java run-potion discard successor still exposes occupied slot {slot}"
        ));
    }
    Ok(())
}

fn ready_mirror_status(
    output_dir: &Path,
    timeout: Duration,
    report: &mut MirrorExecutionReport,
    expected_site: &str,
    expected_action_kind: Option<&str>,
    expected_event_screen_index: Option<i64>,
) -> Result<LiveStatus, String> {
    const MAX_JAVA_ONLY_NORMALIZATIONS: usize = 8;
    for _ in 0..=MAX_JAVA_ONLY_NORMALIZATIONS {
        let view = status(output_dir)?;
        let decision = &view.frame["public_base"]["decision"];
        if decision["kind"].as_str() == Some("ftue") {
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "acknowledge_ftue")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err("FTUE frame did not expose one unique typed acknowledgement".to_owned());
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "acknowledge_ftue", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("event")
            && decision["event_id"].as_str() == Some("Neow")
            && decision["event_screen_index"].as_i64() == Some(0)
            && expected_site == "neow"
            && expected_action_kind == Some("choose_event_option")
            && expected_event_screen_index.is_some_and(|screen| screen != 0)
        {
            let matches = view
                .actions
                .iter()
                .filter(|action| {
                    action.kind == "choose_event_option"
                        && action.option_index == Some(0)
                        && action.event_id.as_deref() == Some("Neow")
                })
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "introductory Neow frame did not expose one unique typed continue action"
                        .to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_neow_intro_continue", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("event")
            && decision["event_id"].as_str() == Some("Neow")
            && decision["event_screen_index"].as_i64() == Some(99)
        {
            let matches = view
                .actions
                .iter()
                .filter(|action| {
                    action.kind == "choose_event_option"
                        && action.option_index == Some(0)
                        && action.event_id.as_deref() == Some("Neow")
                })
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "terminal Neow frame did not expose one unique typed leave action".to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_neow_complete", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("event")
            && expected_site == "map"
            && view.frame["boundary"]["evidence"]["room_phase"].as_str() == Some("COMPLETE")
        {
            let event_id = decision["event_id"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "completed Java event omitted its event identity".to_owned())?;
            let matches = view
                .actions
                .iter()
                .filter(|action| {
                    action.kind == "choose_event_option"
                        && action.option_index == Some(0)
                        && action.event_id.as_deref() == Some(event_id)
                })
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(format!(
                    "completed Java event {event_id:?} did not expose one unique typed leave action"
                ));
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_complete_event", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("combat_reward")
            && !view
                .actions
                .iter()
                .any(|action| action.kind == "claim_combat_reward")
        {
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "close_combat_reward")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "empty Java combat reward did not expose one unique typed close action"
                        .to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(
                report,
                None,
                "java_close_empty_combat_reward",
                None,
                &receipt,
            )?;
            continue;
        }
        if decision["kind"].as_str() == Some("boss_treasure") {
            if expected_site != "boss_relic" {
                return Err(format!(
                    "Java entered a boss treasure room while the retained Rust path expects {expected_site:?}"
                ));
            }
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "open_boss_chest")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "Java boss treasure did not expose one unique typed open action".to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_open_boss_chest", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("boss_treasure_complete") {
            if expected_site != "map" {
                return Err(format!(
                    "Java completed a boss treasure while the retained Rust path expects {expected_site:?}"
                ));
            }
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "proceed_boss_treasure")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "completed Java boss treasure did not expose one unique typed proceed"
                        .to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_proceed_boss_treasure", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("double_boss_complete") {
            if expected_site != "combat" {
                return Err(format!(
                    "Java completed the first A20 boss while the retained Rust path expects {expected_site:?}"
                ));
            }
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "proceed_double_boss")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "completed Java first A20 boss did not expose one unique typed proceed"
                        .to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_proceed_double_boss", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("act3_victory_complete") {
            if expected_site != "map" {
                return Err(format!(
                    "Java completed the Act 3 victory while the retained Rust path expects {expected_site:?}"
                ));
            }
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "proceed_act3_victory")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "completed Java Act 3 victory did not expose one unique typed proceed"
                        .to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_proceed_act3_victory", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("true_victory_complete") {
            if expected_site != "terminal" {
                return Err(format!(
                    "Java completed The Ending boss while the retained Rust path expects {expected_site:?}"
                ));
            }
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "proceed_true_victory")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "completed Java The Ending boss did not expose one unique typed proceed"
                        .to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_proceed_true_victory", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("event")
            && decision["event_id"].as_str() == Some("SpireHeart")
            && expected_site == "map"
        {
            let matches = view
                .actions
                .iter()
                .filter(|action| {
                    action.kind == "choose_event_option"
                        && action.option_index == Some(0)
                        && action.event_id.as_deref() == Some("SpireHeart")
                })
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "Act 3 SpireHeart transition did not expose one unique typed continue"
                        .to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_advance_spire_heart", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("shop_entry") {
            let (action_kind, semantic_action) = match expected_site {
                "shop" => ("enter_shop", "java_enter_shop"),
                "map" => ("proceed_shop_room", "java_proceed_shop_room"),
                other => {
                    return Err(format!(
                        "Java settled in SHOP_ROOM while the retained Rust path expects {other:?}"
                    ));
                }
            };
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == action_kind)
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(format!(
                    "Java SHOP_ROOM did not expose one unique typed {action_kind} action"
                ));
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, semantic_action, None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("campfire") {
            if expected_site == "campfire"
                && matches!(expected_action_kind, Some("rest" | "recall"))
            {
                return Ok(view);
            }
            if expected_site != "campfire" || expected_action_kind != Some("smith") {
                return Err(format!(
                    "Java settled at a campfire while the retained Rust path expects site={expected_site:?} action={expected_action_kind:?}"
                ));
            }
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "begin_campfire_smith")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err("Java campfire did not expose one unique typed smith action".to_owned());
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_begin_campfire_smith", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("smith_confirm") {
            if expected_site != "map" {
                return Err(format!(
                    "Java requires smith confirmation while the retained Rust path expects {expected_site:?}"
                ));
            }
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "confirm_smith_card")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "Java smith preview did not expose one unique typed confirmation".to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_confirm_smith_card", None, &receipt)?;
            continue;
        }
        if decision["kind"].as_str() == Some("campfire_complete") {
            if expected_site != "map" {
                return Err(format!(
                    "Java completed a campfire while the retained Rust path expects {expected_site:?}"
                ));
            }
            let matches = view
                .actions
                .iter()
                .filter(|action| action.kind == "proceed_rest_room")
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(
                    "completed Java campfire did not expose one unique typed proceed".to_owned(),
                );
            };
            let receipt = act(output_dir, &action.action_id, timeout)?;
            record_execution_step(report, None, "java_proceed_rest_room", None, &receipt)?;
            continue;
        }
        return Ok(view);
    }
    Err("mirror run exceeded its bounded Java-only normalizations".to_owned())
}

fn resolve_mirror_action<'a>(
    view: &'a LiveStatus,
    step: &MirrorStep,
) -> Result<&'a ActionHandle, String> {
    let action = &step.selected_candidate["action"];
    let matches = match step.action_kind.as_deref() {
        Some("choose_event_option") if is_library_event_card_choice(step) => {
            resolve_library_grid_select_actions(view, step)?
        }
        Some("choose_event_option") => {
            let option_index = action["option_index"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| "Rust event action omitted option_index".to_owned())?;
            let expected_event = action["event"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Rust event action omitted event identity".to_owned())?;
            let observed_event_ids = view
                .actions
                .iter()
                .filter(|candidate| candidate.kind == "choose_event_option")
                .filter_map(|candidate| candidate.event_id.as_deref())
                .collect::<BTreeSet<_>>();
            if !observed_event_ids
                .iter()
                .any(|event_id| event_ids_match(expected_event, event_id))
            {
                return Err(format!(
                    "current Java event identities {observed_event_ids:?} do not match retained Rust event {expected_event:?} at source depth {}",
                    step.depth
                ));
            }
            view.actions
                .iter()
                .filter(|candidate| {
                    candidate.kind == "choose_event_option"
                        && candidate.option_index == Some(option_index)
                        && candidate
                            .event_id
                            .as_deref()
                            .is_some_and(|event_id| event_ids_match(expected_event, event_id))
                })
                .collect::<Vec<_>>()
        }
        Some("choose_route_node") => {
            let x = action["x"]
                .as_i64()
                .ok_or_else(|| "Rust route action omitted x".to_owned())?;
            let y = action["y"]
                .as_i64()
                .ok_or_else(|| "Rust route action omitted y".to_owned())?;
            view.actions
                .iter()
                .filter(|candidate| {
                    candidate.kind == "choose_map_node"
                        && candidate.map_x == Some(x)
                        && candidate.map_y == Some(y)
                })
                .collect::<Vec<_>>()
        }
        Some("claim_reward") => resolve_claim_reward_actions(view, action)?,
        Some("open_card_reward") => resolve_open_card_reward_actions(view, action)?,
        Some("take_card") => resolve_take_card_reward_actions(view, action)?,
        Some("submit_indexed_choice") if step.site == "combat" => {
            resolve_combat_card_reward_choice_actions(view, step)?
        }
        Some("skip_card_reward") => view
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "skip_card_reward")
            .collect::<Vec<_>>(),
        Some("take_boss_relic") if step.site == "boss_relic" => {
            let option_index = action["option_index"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| "Rust boss relic action omitted option_index".to_owned())?;
            let relic = action["relic"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Rust boss relic action omitted relic ID".to_owned())?;
            view.actions
                .iter()
                .filter(|candidate| {
                    candidate.kind == "choose_boss_relic"
                        && candidate.reward_index == Some(option_index)
                        && candidate
                            .reward_id
                            .as_deref()
                            .is_some_and(|java_id| game_ids_match(relic, java_id))
                })
                .collect::<Vec<_>>()
        }
        Some("skip_boss_relic") if step.site == "boss_relic" => view
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "skip_boss_relic")
            .collect::<Vec<_>>(),
        Some("buy_card") if step.site == "shop" => resolve_buy_shop_card_actions(view, action)?,
        Some("buy_relic") if step.site == "shop" => resolve_buy_shop_relic_actions(view, action)?,
        Some("buy_potion") if step.site == "shop" => resolve_buy_shop_potion_actions(view, action)?,
        Some("leave_shop") if step.site == "shop" => view
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "leave_shop")
            .collect::<Vec<_>>(),
        Some("rest") if step.site == "campfire" => view
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "begin_campfire_rest")
            .collect::<Vec<_>>(),
        Some("recall") if step.site == "campfire" => view
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "begin_campfire_recall")
            .collect::<Vec<_>>(),
        Some("smith") if step.site == "campfire" => resolve_smith_card_actions(view, action)?,
        Some("open_chest") if step.site == "treasure" => view
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "open_chest")
            .collect::<Vec<_>>(),
        Some("discard_run_potion") => resolve_discard_run_potion_actions(view, action)?,
        Some("submit_hand_select") if step.site == "combat" => {
            resolve_hand_select_actions(view, step)?
        }
        Some("submit_grid_select") if step.site == "combat" => {
            resolve_grid_select_actions(view, step)?
        }
        Some("submit_run_selection") if is_supported_event_deck_run_selection(step) => {
            resolve_event_deck_grid_select_actions(view, step)?
        }
        Some("proceed" | "cancel") if step.site == "reward" => view
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "close_combat_reward")
            .collect::<Vec<_>>(),
        Some("end_turn") => view
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "end_turn")
            .collect::<Vec<_>>(),
        Some("play_card") => resolve_card_actions(view, step)?,
        Some("use_potion") => resolve_potion_actions(view, step)?,
        other => {
            return Err(format!(
                "mirror runner has no action resolver for {other:?}"
            ))
        }
    };
    match matches.as_slice() {
        [action] => Ok(*action),
        [] => Err(format!(
            "no current Java action matches {} at source depth {}",
            step.semantic_action, step.depth
        )),
        _ => Err(format!(
            "multiple current Java actions alias {} at source depth {}",
            step.semantic_action, step.depth
        )),
    }
}

fn resolve_hand_select_actions<'a>(
    view: &'a LiveStatus,
    step: &MirrorStep,
) -> Result<Vec<&'a ActionHandle>, String> {
    let selected_uuids = selected_hand_card_uuids(view, step)?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "submit_hand_select"
                && selected_uuids
                    .iter()
                    .all(|uuid| candidate.hand_select_allowed_card_uuids.contains(uuid))
                && candidate
                    .hand_select_allowed_counts
                    .contains(&selected_uuids.len())
        })
        .collect())
}

fn selected_hand_card_uuids(view: &LiveStatus, step: &MirrorStep) -> Result<Vec<String>, String> {
    let candidates = view.frame["public_base"]["decision"]["hand_select"]["candidate_cards"]
        .as_array()
        .ok_or_else(|| "Java HAND_SELECT frame omitted typed candidate cards".to_owned())?;
    selected_hand_card_uuids_from_candidates(candidates, step)
}

fn selected_hand_card_uuids_from_candidates(
    candidates: &[Value],
    step: &MirrorStep,
) -> Result<Vec<String>, String> {
    let ordinal_path = step.choice["ordinal_path"]
        .as_array()
        .ok_or_else(|| "Rust selection-family step omitted choice.ordinal_path".to_owned())?;
    if ordinal_path.is_empty() {
        return Err("Rust selection-family step omitted its root candidate ordinal".to_owned());
    }
    let domain = step.selected_candidate["family"]["domain"]
        .as_array()
        .ok_or_else(|| "Rust selection-family step omitted its typed domain".to_owned())?;
    let mut selected_domain_indices = BTreeSet::new();
    let mut selected_java_uuids = BTreeSet::new();
    let mut result = Vec::with_capacity(ordinal_path.len().saturating_sub(1));
    for raw_index in ordinal_path.iter().skip(1) {
        let selected_index = raw_index
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                "Rust selection-family step contains an invalid selected domain index".to_owned()
            })?;
        if !selected_domain_indices.insert(selected_index) {
            return Err(format!(
                "Rust HAND_SELECT choice repeats typed domain index {selected_index}"
            ));
        }
        let selected = domain.get(selected_index).ok_or_else(|| {
            format!("Rust HAND_SELECT choice index {selected_index} is outside its typed domain")
        })?;
        if selected["kind"].as_str() != Some("card") || selected["eligible"].as_bool() != Some(true)
        {
            return Err(format!(
                "Rust HAND_SELECT choice index {selected_index} is not one eligible card occurrence"
            ));
        }
        let ordinal = selected["ordinal"]
            .as_u64()
            .ok_or_else(|| "Rust selected card omitted its hand ordinal".to_owned())?;
        let card_id = selected["card_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Rust selected card omitted its card ID".to_owned())?;
        let upgrades = selected["upgrades"]
            .as_u64()
            .ok_or_else(|| "Rust selected card omitted upgrades".to_owned())?;
        let matches = candidates
            .iter()
            .filter(|candidate| {
                candidate["index"].as_u64() == Some(ordinal)
                    && candidate["upgrades"].as_u64() == Some(upgrades)
                    && candidate["card_id"]
                        .as_str()
                        .is_some_and(|java_id| game_ids_match(card_id, java_id))
            })
            .collect::<Vec<_>>();
        let [candidate] = matches.as_slice() else {
            return Err(format!(
                "Java HAND_SELECT does not contain one exact match for Rust card {card_id:?} ordinal={ordinal} upgrades={upgrades}"
            ));
        };
        let uuid = candidate["card_uuid"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Java HAND_SELECT candidate omitted card_uuid".to_owned())?
            .to_owned();
        if !selected_java_uuids.insert(uuid.clone()) {
            return Err(format!(
                "Rust HAND_SELECT choices alias Java card UUID {uuid}"
            ));
        }
        result.push(uuid);
    }
    Ok(result)
}

fn resolve_grid_select_actions<'a>(
    view: &'a LiveStatus,
    step: &MirrorStep,
) -> Result<Vec<&'a ActionHandle>, String> {
    let selected_uuid = selected_grid_card_uuid(view, step)?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "submit_grid_select"
                && candidate.grid_select_source_pile.as_deref() == Some("Discard")
                && candidate.grid_select_reason.as_deref() == Some("DiscardToHand")
                && candidate
                    .grid_select_allowed_card_uuids
                    .contains(&selected_uuid)
                && candidate.grid_select_allowed_counts.as_slice() == [1]
        })
        .collect())
}

fn resolve_library_grid_select_actions<'a>(
    view: &'a LiveStatus,
    step: &MirrorStep,
) -> Result<Vec<&'a ActionHandle>, String> {
    let selected_uuid = selected_library_grid_card_uuid(view, step)?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "submit_grid_select"
                && candidate.grid_select_source_pile.as_deref() == Some("EventOffer")
                && candidate.grid_select_reason.as_deref() == Some("TheLibrary")
                && candidate
                    .grid_select_allowed_card_uuids
                    .contains(&selected_uuid)
                && candidate.grid_select_allowed_counts.as_slice() == [1]
        })
        .collect())
}

fn resolve_event_deck_grid_select_actions<'a>(
    view: &'a LiveStatus,
    step: &MirrorStep,
) -> Result<Vec<&'a ActionHandle>, String> {
    let selected_uuid = selected_event_deck_grid_card_uuid(view, step)?;
    let selection_reason = event_deck_grid_selection_reason(step)?;
    let selection = step
        .run_selection
        .as_ref()
        .ok_or_else(|| "event GRID_SELECT step omitted typed selection context".to_owned())?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "submit_grid_select"
                && candidate.grid_select_source_pile.as_deref() == Some("MasterDeck")
                && candidate.grid_select_reason.as_deref() == Some(selection_reason)
                && candidate
                    .event_id
                    .as_deref()
                    .is_some_and(|java_id| event_ids_match(&selection.source_event, java_id))
                && candidate
                    .grid_select_allowed_card_uuids
                    .contains(&selected_uuid)
                && candidate.grid_select_allowed_counts.as_slice() == [1]
        })
        .collect())
}

fn selected_mirror_grid_card_uuid(view: &LiveStatus, step: &MirrorStep) -> Result<String, String> {
    if is_library_event_card_choice(step) {
        selected_library_grid_card_uuid(view, step)
    } else if is_supported_event_deck_run_selection(step) {
        selected_event_deck_grid_card_uuid(view, step)
    } else {
        selected_grid_card_uuid(view, step)
    }
}

fn event_deck_grid_selection_reason(step: &MirrorStep) -> Result<&'static str, String> {
    let selection = step
        .run_selection
        .as_ref()
        .ok_or_else(|| "event GRID_SELECT step omitted typed selection context".to_owned())?;
    match (
        selection.source_event.as_str(),
        selection.selection_kind.as_deref(),
    ) {
        (_, Some("remove_card")) => Ok("DeckRemove"),
        (_, Some("transform_card")) => Ok("DeckTransform"),
        (_, Some("upgrade_card")) => Ok("DeckUpgrade"),
        ("BonfireElementals", None | Some("offer_card")) => Ok("DeckRemove"),
        _ => Err("event GRID_SELECT step has an unsupported typed selection context".to_owned()),
    }
}

fn selected_event_deck_grid_card_uuid(
    view: &LiveStatus,
    step: &MirrorStep,
) -> Result<String, String> {
    let selection = step
        .run_selection
        .as_ref()
        .filter(|_| is_supported_event_deck_run_selection(step))
        .ok_or_else(|| "event GRID_SELECT step omitted typed selection context".to_owned())?;
    let candidates = view.frame["public_base"]["decision"]["grid_select"]["candidate_cards"]
        .as_array()
        .ok_or_else(|| "Java event GRID_SELECT omitted typed candidate cards".to_owned())?;
    let candidate = candidates
        .iter()
        .filter(|candidate| {
            candidate["upgrades"].as_u64() == Some(u64::from(selection.upgrades))
                && candidate["card_id"]
                    .as_str()
                    .is_some_and(|java_id| game_ids_match(&selection.card_id, java_id))
        })
        .nth(selection.occurrence)
        .ok_or_else(|| {
            format!(
                "Java event GRID_SELECT has no occurrence #{} of Rust card {:?} upgrades={}",
                selection.occurrence, selection.card_id, selection.upgrades
            )
        })?;
    candidate["card_uuid"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Java event GRID_SELECT candidate omitted card_uuid".to_owned())
}

fn is_library_event_card_choice(step: &MirrorStep) -> bool {
    step.site == "event"
        && step.action_kind.as_deref() == Some("choose_event_option")
        && step.selected_candidate["action"]["event"].as_str() == Some("TheLibrary")
        && step.selected_candidate["action"]["screen"].as_i64() == Some(1)
}

fn selected_library_grid_card_uuid(view: &LiveStatus, step: &MirrorStep) -> Result<String, String> {
    let gained = step.selected_candidate["action"]["public_semantics"]["specific_cards_gained"]
        .as_array()
        .and_then(|cards| cards.first())
        .ok_or_else(|| "Rust Library card choice omitted its exact gained card".to_owned())?;
    let card_id = gained["card"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust Library card choice omitted its card ID".to_owned())?;
    let upgrades = gained["upgrades"]
        .as_u64()
        .ok_or_else(|| "Rust Library card choice omitted upgrades".to_owned())?;
    let candidates = view.frame["public_base"]["decision"]["grid_select"]["candidate_cards"]
        .as_array()
        .ok_or_else(|| "Java Library GRID_SELECT omitted typed candidate cards".to_owned())?;
    let matches = candidates
        .iter()
        .filter(|candidate| {
            candidate["upgrades"].as_u64() == Some(upgrades)
                && candidate["card_id"]
                    .as_str()
                    .is_some_and(|java_id| game_ids_match(card_id, java_id))
        })
        .collect::<Vec<_>>();
    let [candidate] = matches.as_slice() else {
        return Err(format!(
            "Java Library GRID_SELECT does not contain one exact match for Rust card {card_id:?} upgrades={upgrades}"
        ));
    };
    candidate["card_uuid"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Java Library GRID_SELECT candidate omitted card_uuid".to_owned())
}

fn selected_grid_card_uuid(view: &LiveStatus, step: &MirrorStep) -> Result<String, String> {
    if step.selected_candidate["family"]["source_pile"].as_str() != Some("Discard")
        || step.selected_candidate["family"]["reason"]["kind"].as_str() != Some("grid")
        || step.selected_candidate["family"]["reason"]["value"].as_str() != Some("DiscardToHand")
    {
        return Err("Rust GRID_SELECT step is outside the DiscardToHand scope".to_owned());
    }
    let ordinal_path = step.choice["ordinal_path"]
        .as_array()
        .ok_or_else(|| "Rust selection-family step omitted choice.ordinal_path".to_owned())?;
    let selected_index = ordinal_path
        .last()
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust selection-family step omitted its selected domain index".to_owned())?;
    let domain = step.selected_candidate["family"]["domain"]
        .as_array()
        .ok_or_else(|| "Rust selection-family step omitted its typed domain".to_owned())?;
    let selected = domain
        .get(selected_index)
        .ok_or_else(|| "Rust selection-family choice is outside its typed domain".to_owned())?;
    if selected["kind"].as_str() != Some("card") || selected["eligible"].as_bool() != Some(true) {
        return Err("Rust selection-family choice is not one eligible card occurrence".to_owned());
    }
    let ordinal = selected["ordinal"]
        .as_u64()
        .ok_or_else(|| "Rust selected card omitted its source-pile ordinal".to_owned())?;
    let card_id = selected["card_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust selected card omitted its card ID".to_owned())?;
    let upgrades = selected["upgrades"]
        .as_u64()
        .ok_or_else(|| "Rust selected card omitted upgrades".to_owned())?;
    let candidates = view.frame["public_base"]["decision"]["grid_select"]["candidate_cards"]
        .as_array()
        .ok_or_else(|| "Java GRID_SELECT frame omitted typed candidate cards".to_owned())?;
    let matches = candidates
        .iter()
        .filter(|candidate| {
            candidate["index"].as_u64() == Some(ordinal)
                && candidate["upgrades"].as_u64() == Some(upgrades)
                && candidate["card_id"]
                    .as_str()
                    .is_some_and(|java_id| game_ids_match(card_id, java_id))
        })
        .collect::<Vec<_>>();
    let [candidate] = matches.as_slice() else {
        return Err(format!(
            "Java GRID_SELECT does not contain one exact match for Rust card {card_id:?} ordinal={ordinal} upgrades={upgrades}"
        ));
    };
    candidate["card_uuid"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Java GRID_SELECT candidate omitted card_uuid".to_owned())
}

fn resolve_potion_actions<'a>(
    view: &'a LiveStatus,
    step: &MirrorStep,
) -> Result<Vec<&'a ActionHandle>, String> {
    let action = &step.selected_candidate["action"];
    let potion_slot = action["potion_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust use-potion action omitted potion_index".to_owned())?;
    let rust_target = action["target_monster_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok());
    let target = resolve_java_target(view, step, rust_target)?;
    let expected_potion_id = step
        .combat_before
        .as_ref()
        .and_then(|combat| combat.potions.get(potion_slot))
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            format!("Rust combat projection has no potion identity in selected slot {potion_slot}")
        })?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "use_potion"
                && candidate.potion_slot == Some(potion_slot)
                && candidate
                    .potion_id
                    .as_deref()
                    .is_some_and(|id| game_ids_match(expected_potion_id, id))
                && target_matches(candidate, target.as_ref())
        })
        .collect())
}

#[derive(Debug)]
struct JavaTargetIdentity {
    index: usize,
    instance_id: u64,
    monster_id: String,
}

fn resolve_java_target(
    view: &LiveStatus,
    step: &MirrorStep,
    rust_target_index: Option<usize>,
) -> Result<Option<JavaTargetIdentity>, String> {
    let Some(rust_target_index) = rust_target_index else {
        return Ok(None);
    };
    let combat = step
        .combat_before
        .as_ref()
        .ok_or_else(|| "targeted Rust combat step omitted combat_before".to_owned())?;
    resolve_java_target_from_frame(&view.frame, combat, rust_target_index)
}

fn resolve_java_target_from_frame(
    frame: &Value,
    combat: &MirrorCombatProjection,
    rust_target_index: usize,
) -> Result<Option<JavaTargetIdentity>, String> {
    let expected = combat.monsters.get(rust_target_index).ok_or_else(|| {
        format!("Rust target index {rust_target_index} is outside its typed combat projection")
    })?;
    if expected.source_index != rust_target_index {
        return Err(format!(
            "Rust target projection index {} disagrees with source_index {}",
            rust_target_index, expected.source_index
        ));
    }
    let java_monsters = frame["public_base"]["combat"]["monsters"]
        .as_array()
        .ok_or_else(|| "Java combat frame omitted monsters".to_owned())?;
    let mut same_enemy_ordinal = 0usize;
    for (array_index, monster) in java_monsters.iter().enumerate() {
        let Some(monster_id) = monster["id"].as_str() else {
            return Err(format!("Java monster {array_index} omitted its ID"));
        };
        if !enemy_ids_match(&expected.enemy_id, monster_id) {
            continue;
        }
        if same_enemy_ordinal == expected.same_enemy_ordinal {
            let index = monster["index"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| format!("Java monster {array_index} omitted its group index"))?;
            let instance_id = monster["instance_id"].as_u64().ok_or_else(|| {
                format!("Java monster {array_index} omitted its runtime instance ID")
            })?;
            if index != array_index {
                return Err(format!(
                    "Java monster array index {array_index} disagrees with typed group index {index}"
                ));
            }
            return Ok(Some(JavaTargetIdentity {
                index,
                instance_id,
                monster_id: monster_id.to_owned(),
            }));
        }
        same_enemy_ordinal += 1;
    }
    Err(format!(
        "Java combat has no {:?} occurrence {} for Rust target index {}",
        expected.enemy_id, expected.same_enemy_ordinal, rust_target_index
    ))
}

fn target_matches(candidate: &ActionHandle, expected: Option<&JavaTargetIdentity>) -> bool {
    match expected {
        None => {
            candidate.target_monster_index.is_none()
                && candidate.target_monster_instance_id.is_none()
                && candidate.target_monster_id.is_none()
        }
        Some(expected) => {
            candidate.target_monster_index == Some(expected.index)
                && candidate.target_monster_instance_id == Some(expected.instance_id)
                && candidate
                    .target_monster_id
                    .as_deref()
                    .is_some_and(|id| enemy_ids_match(&expected.monster_id, id))
        }
    }
}

fn resolve_claim_reward_actions<'a>(
    view: &'a LiveStatus,
    action: &Value,
) -> Result<Vec<&'a ActionHandle>, String> {
    let reward_index = action["reward_item_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust claim-reward action omitted reward_item_index".to_owned())?;
    let reward = &action["reward"];
    match reward["kind"].as_str() {
        Some("gold") | Some("stolen_gold") => {
            let amount = reward["amount"]
                .as_i64()
                .ok_or_else(|| "Rust gold reward omitted amount".to_owned())?;
            Ok(view
                .actions
                .iter()
                .filter(|candidate| {
                    candidate.kind == "claim_combat_reward"
                        && candidate.reward_index == Some(reward_index)
                        && matches!(
                            candidate.reward_type.as_deref(),
                            Some("GOLD" | "STOLEN_GOLD")
                        )
                        && candidate.reward_gold == Some(amount)
                })
                .collect())
        }
        Some("potion") => {
            let potion = reward["potion"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Rust potion reward omitted potion ID".to_owned())?;
            Ok(view
                .actions
                .iter()
                .filter(|candidate| {
                    candidate.kind == "claim_combat_reward"
                        && candidate.reward_index == Some(reward_index)
                        && candidate.reward_type.as_deref() == Some("POTION")
                        && candidate
                            .reward_id
                            .as_deref()
                            .is_some_and(|java_id| game_ids_match(potion, java_id))
                })
                .collect())
        }
        Some("relic") => {
            let relic = reward["relic"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Rust relic reward omitted relic ID".to_owned())?;
            Ok(view
                .actions
                .iter()
                .filter(|candidate| {
                    candidate.kind == "claim_combat_reward"
                        && candidate.reward_index == Some(reward_index)
                        && candidate.reward_type.as_deref() == Some("RELIC")
                        && candidate
                            .reward_id
                            .as_deref()
                            .is_some_and(|java_id| game_ids_match(relic, java_id))
                })
                .collect())
        }
        Some("emerald_key") => Ok(view
            .actions
            .iter()
            .filter(|candidate| {
                candidate.kind == "claim_combat_reward"
                    && candidate.reward_index == Some(reward_index)
                    && candidate.reward_type.as_deref() == Some("EMERALD_KEY")
                    && candidate.reward_id.as_deref() == Some("EMERALD_KEY")
            })
            .collect()),
        Some("sapphire_key") => Ok(view
            .actions
            .iter()
            .filter(|candidate| {
                candidate.kind == "claim_combat_reward"
                    && candidate.reward_index == Some(reward_index)
                    && candidate.reward_type.as_deref() == Some("SAPPHIRE_KEY")
            })
            .collect()),
        other => Err(format!(
            "mirror runner does not yet resolve claimed reward kind {other:?}"
        )),
    }
}

fn mirror_run_contract_error(objective: &str, frame: &Value) -> Option<String> {
    if objective != "heart_victory" {
        return None;
    }
    match frame["public_base"]["run"]["final_act_available"].as_bool() {
        Some(true) => None,
        Some(false) => Some(
            "heart_victory requires the original game Final Act flag, but Java reported false"
                .to_owned(),
        ),
        None => Some(
            "heart_victory requires public_base.run.final_act_available, but the Java frame omitted it"
                .to_owned(),
        ),
    }
}

fn resolve_open_card_reward_actions<'a>(
    view: &'a LiveStatus,
    action: &Value,
) -> Result<Vec<&'a ActionHandle>, String> {
    let reward_index = action["reward_item_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust open-card action omitted reward_item_index".to_owned())?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "claim_combat_reward"
                && candidate.reward_index == Some(reward_index)
                && candidate.reward_type.as_deref() == Some("CARD")
        })
        .collect())
}

fn resolve_take_card_reward_actions<'a>(
    view: &'a LiveStatus,
    action: &Value,
) -> Result<Vec<&'a ActionHandle>, String> {
    let option_index = action["option_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust take-card action omitted option_index".to_owned())?;
    let card_id = action["card"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust take-card action omitted card ID".to_owned())?;
    let upgrades = action["upgrades"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust take-card action omitted upgrades".to_owned())?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "choose_card_reward"
                && candidate.reward_index == Some(option_index)
                && candidate.card_upgrades == Some(upgrades)
                && candidate
                    .card_id
                    .as_deref()
                    .is_some_and(|java_id| game_ids_match(card_id, java_id))
        })
        .collect())
}

fn resolve_combat_card_reward_choice_actions<'a>(
    view: &'a LiveStatus,
    step: &MirrorStep,
) -> Result<Vec<&'a ActionHandle>, String> {
    let expected_source_kind = retained_combat_card_reward_source_kind(&step.selected_candidate)
        .ok_or_else(|| {
            format!(
                "retained indexed choice at source depth {} is not a closed combat card-reward family",
                step.depth
            )
        })?;
    let actual_source_kind =
        view.frame["public_base"]["decision"]["card_reward"]["source_kind"].as_str();
    if actual_source_kind != Some(expected_source_kind) {
        return Err(format!(
            "Java CARD_REWARD source kind {:?} at source depth {} does not match retained {:?}",
            actual_source_kind, step.depth, expected_source_kind
        ));
    }
    let choice_index = step.selected_candidate["action"]["choice_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust combat card-reward action omitted choice_index".to_owned())?;
    let card_id = step.selected_candidate["candidate"]["card_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust combat card-reward candidate omitted card ID".to_owned())?;
    let upgrades = step.selected_candidate["candidate"]["upgrades"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust combat card-reward candidate omitted upgrades".to_owned())?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "choose_card_reward"
                && candidate.reward_index == Some(choice_index)
                && candidate.card_upgrades == Some(upgrades)
                && candidate
                    .card_id
                    .as_deref()
                    .is_some_and(|java_id| game_ids_match(card_id, java_id))
        })
        .collect())
}

fn resolve_discard_run_potion_actions<'a>(
    view: &'a LiveStatus,
    action: &Value,
) -> Result<Vec<&'a ActionHandle>, String> {
    let slot = action["slot"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust run-potion discard omitted slot".to_owned())?;
    let potion_id = action["potion"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust run-potion discard omitted potion ID".to_owned())?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "discard_potion"
                && candidate.potion_slot == Some(slot)
                && candidate
                    .potion_id
                    .as_deref()
                    .is_some_and(|java_id| game_ids_match(potion_id, java_id))
        })
        .collect())
}

fn resolve_buy_shop_card_actions<'a>(
    view: &'a LiveStatus,
    action: &Value,
) -> Result<Vec<&'a ActionHandle>, String> {
    let shop_slot = action["shop_slot"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust buy-card action omitted shop_slot".to_owned())?;
    let card_id = action["card"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust buy-card action omitted card ID".to_owned())?;
    let upgrades = action["upgrades"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust buy-card action omitted upgrades".to_owned())?;
    let price = action["price"]
        .as_i64()
        .ok_or_else(|| "Rust buy-card action omitted price".to_owned())?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "buy_shop_card"
                && candidate.shop_slot == Some(shop_slot)
                && candidate.shop_price == Some(price)
                && candidate.card_upgrades == Some(upgrades)
                && candidate
                    .card_id
                    .as_deref()
                    .is_some_and(|java_id| game_ids_match(card_id, java_id))
        })
        .collect())
}

fn resolve_buy_shop_relic_actions<'a>(
    view: &'a LiveStatus,
    action: &Value,
) -> Result<Vec<&'a ActionHandle>, String> {
    let shop_slot = action["shop_slot"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust buy-relic action omitted shop_slot".to_owned())?;
    let relic_id = action["relic"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust buy-relic action omitted relic ID".to_owned())?;
    let price = action["price"]
        .as_i64()
        .filter(|value| *value >= 0)
        .ok_or_else(|| "Rust buy-relic action omitted non-negative price".to_owned())?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "buy_shop_relic"
                && candidate.shop_slot == Some(shop_slot)
                && candidate.shop_price == Some(price)
                && candidate
                    .reward_id
                    .as_deref()
                    .is_some_and(|java_id| game_ids_match(relic_id, java_id))
        })
        .collect())
}

fn resolve_buy_shop_potion_actions<'a>(
    view: &'a LiveStatus,
    action: &Value,
) -> Result<Vec<&'a ActionHandle>, String> {
    let shop_slot = action["shop_slot"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust buy-potion action omitted shop_slot".to_owned())?;
    let potion_id = action["potion"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust buy-potion action omitted potion ID".to_owned())?;
    let price = action["price"]
        .as_i64()
        .filter(|value| *value >= 0)
        .ok_or_else(|| "Rust buy-potion action omitted non-negative price".to_owned())?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "buy_shop_potion"
                && candidate.shop_slot == Some(shop_slot)
                && candidate.shop_price == Some(price)
                && candidate
                    .potion_id
                    .as_deref()
                    .is_some_and(|java_id| game_ids_match(potion_id, java_id))
        })
        .collect())
}

fn resolve_smith_card_actions<'a>(
    view: &'a LiveStatus,
    action: &Value,
) -> Result<Vec<&'a ActionHandle>, String> {
    let card_ordinal = action["card_ordinal"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust smith action omitted card_ordinal".to_owned())?;
    let card_id = action["card"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Rust smith action omitted card ID".to_owned())?;
    let upgrades = action["upgrades"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust smith action omitted upgrades".to_owned())?;
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "choose_smith_card"
                && candidate.card_ordinal == Some(card_ordinal)
                && candidate.card_upgrades == Some(upgrades)
                && candidate
                    .card_id
                    .as_deref()
                    .is_some_and(|java_id| game_ids_match(card_id, java_id))
        })
        .collect())
}

fn power_ids_match(left: &str, right: &str) -> bool {
    fn canonical(value: &str) -> String {
        // The original game's WeakPower exposes "Weakened" as POWER_ID,
        // while the simulator's typed PowerId variant is named "Weak".
        // Darkling's RegrowPower likewise exposes "Life Link" as POWER_ID,
        // and RegenerateMonsterPower exposes "Regenerate" while the simulator
        // names its typed PowerId variant "Regen".
        let value = match value {
            "Weak" | "Weakened" => "Weakened",
            "Regrow" | "Life Link" => "Life Link",
            "Regen" | "Regenerate" => "Regenerate",
            other => other,
        };
        value
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }

    canonical(left) == canonical(right)
}

fn enemy_ids_match(left: &str, right: &str) -> bool {
    fn canonical(value: &str) -> String {
        // Some original monster classes expose legacy runtime IDs that differ
        // from the simulator's class-oriented identity.
        let value = match value {
            "BanditPointy" | "BanditChild" => "BanditChild",
            "LouseDefensive" | "FuzzyLouseDefensive" => "FuzzyLouseDefensive",
            "LouseNormal" | "FuzzyLouseNormal" => "FuzzyLouseNormal",
            // SpireGrowth.ID is "Serpent" in the original PC V2.3.4 source.
            "SpireGrowth" | "Serpent" => "Serpent",
            other => other,
        };
        canonical_game_id(value)
    }

    canonical(left) == canonical(right)
}

fn event_ids_match(rust_event: &str, java_event_id: &str) -> bool {
    rust_event == java_event_id
}

fn resolve_card_actions<'a>(
    view: &'a LiveStatus,
    step: &MirrorStep,
) -> Result<Vec<&'a ActionHandle>, String> {
    let expected_hand_index = step.selected_candidate["action"]["hand_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Rust play-card action omitted hand_index".to_owned())?;
    let rust_target = step.selected_candidate["action"]["target_monster_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok());
    let expected_target = resolve_java_target(view, step, rust_target)?;
    let expected_card = step
        .combat_before
        .as_ref()
        .and_then(|combat| combat.hand.get(expected_hand_index))
        .ok_or_else(|| {
            format!(
                "Rust combat projection has no hand identity at selected index {expected_hand_index}"
            )
        })?;
    let hand = view.frame["public_base"]["combat"]["hand"]
        .as_array()
        .ok_or_else(|| "Java combat frame omitted hand".to_owned())?;
    let Some(card) = hand.get(expected_hand_index) else {
        return Ok(Vec::new());
    };
    if !card["card_id"]
        .as_str()
        .is_some_and(|observed_id| game_ids_match(&expected_card.card_id, observed_id))
        || card["upgrades"].as_u64() != Some(u64::from(expected_card.upgrades))
    {
        return Ok(Vec::new());
    }
    Ok(view
        .actions
        .iter()
        .filter(|candidate| {
            candidate.kind == "play_card"
                && candidate.hand_index == Some(expected_hand_index)
                && candidate
                    .card_id
                    .as_deref()
                    .is_some_and(|observed_id| game_ids_match(&expected_card.card_id, observed_id))
                && target_matches(candidate, expected_target.as_ref())
        })
        .collect())
}

fn compare_scalars(view: &LiveStatus, expected: &MirrorPublicScalars) -> MirrorScalarComparison {
    let run = &view.frame["public_base"]["run"];
    let combat_player = &view.frame["public_base"]["combat"]["player"];
    let hp = combat_player["current_hp"]
        .as_i64()
        .or_else(|| run["current_hp"].as_i64());
    let max_hp = combat_player["max_hp"]
        .as_i64()
        .or_else(|| run["max_hp"].as_i64());
    MirrorScalarComparison {
        act: scalar_field(expected.act, run["act"].as_i64()),
        floor: scalar_field(expected.floor, run["floor"].as_i64()),
        current_hp: scalar_field(expected.current_hp, hp),
        max_hp: scalar_field(expected.max_hp, max_hp),
        gold: scalar_field(expected.gold, run["gold"].as_i64()),
    }
}

fn compare_combat_projection(frame: &Value, expected: &MirrorCombatProjection) -> Option<String> {
    let combat = &frame["public_base"]["combat"];
    let internal = &frame["parity_internal"];
    if !combat.is_object() {
        return Some("Java frame omitted the typed public combat projection".to_owned());
    }
    let mut differences = Vec::<String>::new();
    let mut difference = |detail: String| {
        if differences.len() < 16 {
            differences.push(detail);
        }
    };

    let expected_java_turn = u64::from(expected.turn_index) + 1;
    if combat["turn"].as_u64() != Some(expected_java_turn) {
        difference(format!(
            "turn expected_java={expected_java_turn} actual={:?}",
            combat["turn"].as_u64()
        ));
    }
    if combat["player"]["energy"].as_u64() != Some(u64::from(expected.energy)) {
        difference(format!(
            "energy expected={} actual={:?}",
            expected.energy,
            combat["player"]["energy"].as_u64()
        ));
    }
    if combat["player"]["block"].as_i64() != Some(i64::from(expected.player_block)) {
        difference(format!(
            "player_block expected={} actual={:?}",
            expected.player_block,
            combat["player"]["block"].as_i64()
        ));
    }

    match combat["hand"].as_array() {
        None => difference("Java combat hand is unavailable".to_owned()),
        Some(actual) => {
            if actual.len() != expected.hand.len() {
                difference(format!(
                    "hand length expected={} actual={}",
                    expected.hand.len(),
                    actual.len()
                ));
            }
            for (index, (expected_card, actual_card)) in
                expected.hand.iter().zip(actual).enumerate()
            {
                let id_matches = actual_card["card_id"]
                    .as_str()
                    .is_some_and(|id| game_ids_match(&expected_card.card_id, id));
                let upgrades_match =
                    actual_card["upgrades"].as_u64() == Some(u64::from(expected_card.upgrades));
                let cost_matches = actual_card["cost_for_turn"].as_i64()
                    == Some(i64::from(expected_card.cost_for_turn));
                if !id_matches || !upgrades_match || !cost_matches {
                    difference(format!(
                        "hand[{index}] expected={:?}+{}@{} actual={:?}+{:?}@{:?}",
                        expected_card.card_id,
                        expected_card.upgrades,
                        expected_card.cost_for_turn,
                        actual_card["card_id"].as_str(),
                        actual_card["upgrades"].as_u64(),
                        actual_card["cost_for_turn"].as_i64()
                    ));
                }
            }
        }
    }

    let actual_potions = combat["potions"].as_array();
    for (slot, expected_potion) in expected.potions.iter().enumerate() {
        let actual_potion = actual_potions.and_then(|potions| {
            potions
                .iter()
                .find(|potion| potion["potion_slot"].as_u64() == Some(slot as u64))
        });
        let actual_id = actual_potion.and_then(|potion| {
            (!potion["empty"].as_bool().unwrap_or(false))
                .then(|| potion["potion_id"].as_str())
                .flatten()
        });
        let matches = match (expected_potion.as_deref(), actual_id) {
            (None, None) => true,
            (Some(expected_id), Some(actual_id)) => game_ids_match(expected_id, actual_id),
            _ => false,
        };
        if !matches {
            difference(format!(
                "potion[{slot}] expected={expected_potion:?} actual={actual_id:?}"
            ));
        }
    }

    compare_power_projection(
        "player_powers",
        &expected.player_powers,
        internal["player_powers"].as_array(),
        &mut difference,
    );

    match combat["monsters"].as_array() {
        None => difference("Java combat monsters are unavailable".to_owned()),
        Some(actual_monsters) => {
            if actual_monsters.len() != expected.monsters.len() {
                difference(format!(
                    "monster length expected={} actual={}",
                    expected.monsters.len(),
                    actual_monsters.len()
                ));
            }
            let internal_monsters = internal["monsters"].as_array();
            let mut actual_same_enemy_counts = BTreeMap::<String, usize>::new();
            for (index, (expected_monster, actual_monster)) in
                expected.monsters.iter().zip(actual_monsters).enumerate()
            {
                let actual_id = actual_monster["id"].as_str();
                let id_matches =
                    actual_id.is_some_and(|id| enemy_ids_match(&expected_monster.enemy_id, id));
                let actual_ordinal = actual_id.map(|id| {
                    let canonical = canonical_game_id(id);
                    let ordinal = *actual_same_enemy_counts
                        .entry(canonical.clone())
                        .or_default();
                    *actual_same_enemy_counts.get_mut(&canonical).unwrap() += 1;
                    ordinal
                });
                if expected_monster.source_index != index
                    || !id_matches
                    || actual_ordinal != Some(expected_monster.same_enemy_ordinal)
                    || actual_monster["current_hp"].as_i64() != Some(i64::from(expected_monster.hp))
                    || actual_monster["max_hp"].as_i64() != Some(i64::from(expected_monster.max_hp))
                    || actual_monster["block"].as_i64() != Some(i64::from(expected_monster.block))
                    || actual_monster["dead_or_escaped"].as_bool() != Some(!expected_monster.alive)
                {
                    difference(format!(
                        "monster[{index}] expected={}#{} {}/{} b{} alive={} actual={:?}#{actual_ordinal:?} {:?}/{:?} b{:?} dead_or_escaped={:?}",
                        expected_monster.enemy_id,
                        expected_monster.same_enemy_ordinal,
                        expected_monster.hp,
                        expected_monster.max_hp,
                        expected_monster.block,
                        expected_monster.alive,
                        actual_id,
                        actual_monster["current_hp"].as_i64(),
                        actual_monster["max_hp"].as_i64(),
                        actual_monster["block"].as_i64(),
                        actual_monster["dead_or_escaped"].as_bool(),
                    ));
                }
                // The original game keeps a dying monster object (including its
                // last intent and powers) until the render-timed death animation
                // disposes it. Once both projections agree that the slot is dead
                // or escaped, those fields cannot participate in later actions.
                if !expected_monster.alive
                    && actual_monster["dead_or_escaped"].as_bool() == Some(true)
                {
                    continue;
                }
                let actual_internal = internal_monsters.and_then(|monsters| monsters.get(index));
                let actual_intent = actual_internal
                    .and_then(|monster| monster["intent"].as_str())
                    .map(canonical_game_id);
                if let Some(expected_intent) =
                    expected_monster.intent.as_deref().map(canonical_game_id)
                {
                    if actual_intent.as_deref() != Some(expected_intent.as_str()) {
                        difference(format!(
                            "monster[{index}].intent expected={expected_intent:?} actual={actual_intent:?}"
                        ));
                    }
                }
                compare_power_projection(
                    &format!("monster[{index}].powers"),
                    &expected_monster.powers,
                    actual_internal.and_then(|monster| monster["powers"].as_array()),
                    &mut difference,
                );
            }
        }
    }

    (!differences.is_empty()).then(|| {
        format!(
            "Java combat projection first differs from retained Rust state: {}",
            differences.join("; ")
        )
    })
}

fn compare_power_projection(
    scope: &str,
    expected: &[MirrorCombatPower],
    actual: Option<&Vec<Value>>,
    difference: &mut impl FnMut(String),
) {
    let Some(actual) = actual else {
        if !expected.is_empty() {
            difference(format!(
                "{scope} unavailable; expected {} powers",
                expected.len()
            ));
        }
        return;
    };
    if actual.len() != expected.len() {
        difference(format!(
            "{scope} length expected={} actual={}",
            expected.len(),
            actual.len()
        ));
    }
    for (index, (expected_power, actual_power)) in expected.iter().zip(actual).enumerate() {
        if !actual_power["id"]
            .as_str()
            .is_some_and(|id| power_ids_match(&expected_power.power_id, id))
            || actual_power["amount"].as_i64() != Some(i64::from(expected_power.amount))
        {
            difference(format!(
                "{scope}[{index}] expected={}:{} actual={:?}:{:?}",
                expected_power.power_id,
                expected_power.amount,
                actual_power["id"].as_str(),
                actual_power["amount"].as_i64()
            ));
        }
    }
}

fn scalar_field(expected: i64, actual: Option<i64>) -> MirrorScalarField {
    MirrorScalarField {
        status: match actual {
            Some(value) if value == expected => "matched",
            Some(_) => "mismatch",
            None => "unavailable",
        }
        .to_owned(),
        expected,
        actual,
    }
}

fn scalar_comparison_has_mismatch(comparison: &MirrorScalarComparison) -> bool {
    [
        &comparison.act,
        &comparison.floor,
        &comparison.current_hp,
        &comparison.max_hp,
        &comparison.gold,
    ]
    .iter()
    .any(|field| field.status == "mismatch")
}

fn record_execution_step(
    report: &mut MirrorExecutionReport,
    source_depth: Option<usize>,
    semantic_action: &str,
    before_scalars: Option<MirrorScalarComparison>,
    receipt: &ActionReceipt,
) -> Result<(), String> {
    let successor_frame_id = receipt
        .successor_frame_id
        .ok_or_else(|| "completed mirror action omitted successor_frame_id".to_owned())?;
    let receipt_path = Path::new(&report.session.session_dir)
        .join("receipts")
        .join(format!("{}.json", receipt.request_id));
    report.steps.push(MirrorExecutionStep {
        source_depth,
        semantic_action: semantic_action.to_owned(),
        frame_id: receipt.based_on_frame_id,
        action_id: receipt.action_id.clone(),
        successor_frame_id,
        receipt: file_identity(&receipt_path)?,
        before_scalars,
    });
    Ok(())
}

fn stop_with_mismatch<T>(
    report: &mut MirrorExecutionReport,
    mismatch: MirrorMismatch,
) -> Result<T, String> {
    let detail = mismatch.detail.clone();
    report.first_mismatch = Some(mismatch);
    Err(detail)
}

fn validate_mirror_initial_surface(
    view: &LiveStatus,
    expected_launch_plan: &Path,
) -> Result<(), String> {
    if view.frame["boundary"]["kind"].as_str() != Some("main_menu_ready") {
        return Err("mirror run requires a fresh typed main-menu frame".to_owned());
    }
    if view.manifest.profile != "parity_run_control" {
        return Err("mirror run requires parity_run_control".to_owned());
    }
    let launch = view
        .manifest
        .launch
        .as_ref()
        .ok_or_else(|| "mirror run requires launch evidence".to_owned())?;
    if launch.profile != "accelerated"
        || launch.control_profile != "parity_run_control"
        || launch.storage_class != "isolated_working_directory_v1"
        || Path::new(&launch.plan_path) != expected_launch_plan
    {
        return Err("mirror run does not match its accelerated isolated launch plan".to_owned());
    }
    Ok(())
}

fn read_mirror_plan(path: &Path) -> Result<MirrorPlan, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read mirror plan {}: {error}", path.display()))?;
    let plan: MirrorPlan = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse mirror plan {}: {error}", path.display()))?;
    let contract_matches = (plan.schema == MIRROR_PLAN_SCHEMA_V2
        && plan.scope.kind == "neow_through_first_reward_to_map")
        || (plan.schema == MIRROR_PLAN_SCHEMA_V3
            && plan.scope.kind == "neow_through_active_prefix")
        || (plan.schema == MIRROR_PLAN_SCHEMA_V4
            && matches!(
                plan.scope.kind.as_str(),
                "neow_through_first_reward_to_map" | "neow_through_active_prefix"
            ));
    if !contract_matches
        || !plan
            .coverage
            .selected_scope_supported_by_current_run_profile
        || plan.steps.is_empty()
    {
        return Err("mirror plan does not satisfy a recognized execution contract".to_owned());
    }
    if matches!(
        plan.schema.as_str(),
        MIRROR_PLAN_SCHEMA_V3 | MIRROR_PLAN_SCHEMA_V4
    ) && plan.scope.expected_successor_site == "combat"
        && plan
            .scope
            .expected_successor_monster_ids
            .as_ref()
            .is_none_or(Vec::is_empty)
    {
        return Err("active-prefix combat mirror plan omitted successor monster IDs".to_owned());
    }
    match (
        plan.scope.expected_successor_site.as_str(),
        plan.scope.expected_successor_combat_boundary.as_deref(),
    ) {
        ("combat", None | Some(COMBAT_BOUNDARY_PLAYER_DECISION)) => {}
        ("combat", Some(COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD))
            if plan.schema == MIRROR_PLAN_SCHEMA_V4 => {}
        ("combat", Some(_)) => {
            return Err("combat mirror plan names an unknown successor boundary".to_owned())
        }
        (_, None) => {}
        (_, Some(_)) => {
            return Err("non-combat mirror plan carries a combat successor boundary".to_owned())
        }
    }
    match (
        plan.scope.expected_successor_site.as_str(),
        plan.scope.expected_successor_scalars.as_ref(),
        plan.scope.expected_successor_combat.as_ref(),
    ) {
        ("combat", Some(_), Some(_)) | ("combat", None, None) => {}
        ("combat", _, _) => {
            return Err(
                "combat mirror plan carries only part of its additive successor projection"
                    .to_owned(),
            )
        }
        (_, None, None) => {}
        (_, _, _) => {
            return Err("non-combat mirror plan carries a combat successor projection".to_owned())
        }
    }
    Ok(plan)
}

fn validate_current_mirror_plan(plan: &MirrorPlan) -> Result<(), String> {
    if plan.schema != MIRROR_PLAN_SCHEMA_V4 {
        return Err(
            "mirror execution requires a current V4 plan with typed per-action combat projections"
                .to_owned(),
        );
    }
    if plan
        .steps
        .iter()
        .any(|step| step.site == "combat" && step.combat_before.is_none())
    {
        return Err("V4 mirror plan contains a combat step without combat_before".to_owned());
    }
    java_seed_token_for_engine_seed(&plan.run_start.seed)?;
    Ok(())
}

/// Native checkpoints predate the additive per-action combat projection. Their
/// Java reachability claim binds the exact typed action prefix, not a Rust
/// comparison that was never performed. A candidate with its own typed action
/// is already self-identifying, so deliberately exclude its UI ordinal path;
/// candidate ordering may change without changing the submitted action. Keep
/// the path only for selection families where it identifies the chosen member.
/// This preserves the original prefix contract while letting a current target
/// plan add honest forward-only diagnostics.
fn checkpoint_prefix_step_projection(step: &MirrorStep) -> Value {
    json!({
        "depth": step.depth,
        "site": step.site,
        "before": step.before,
        "semantic_action": step.semantic_action,
        "candidate_domain": step.candidate_domain,
        "action_kind": step.action_kind,
        "selected_candidate": step.selected_candidate,
        "selection_choice": (!step.selected_candidate["action"].is_object())
            .then_some(&step.choice),
    })
}

fn effective_combat_boundary(scope: &MirrorScope) -> &str {
    scope
        .expected_successor_combat_boundary
        .as_deref()
        .unwrap_or(COMBAT_BOUNDARY_PLAYER_DECISION)
}

fn java_successor_matches_scope(view: &LiveStatus, scope: &MirrorScope) -> bool {
    if scope.expected_successor_site != "combat" {
        return java_decision_matches_rust_site(view, &scope.expected_successor_site);
    }
    match effective_combat_boundary(scope) {
        COMBAT_BOUNDARY_PLAYER_DECISION => {
            java_frame_matches_combat_boundary(&view.frame, COMBAT_BOUNDARY_PLAYER_DECISION)
        }
        COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD => {
            java_frame_matches_combat_boundary(&view.frame, COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD)
                && java_toolbox_overlay_actions_match(&view.frame, &view.actions)
        }
        _ => false,
    }
}

fn java_frame_matches_combat_boundary(frame: &Value, expected_boundary: &str) -> bool {
    match expected_boundary {
        COMBAT_BOUNDARY_PLAYER_DECISION => {
            frame["boundary"]["kind"].as_str() == Some("combat_player_decision")
        }
        COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD => java_frame_is_initial_toolbox_overlay(frame),
        _ => false,
    }
}

fn java_frame_is_initial_toolbox_overlay(frame: &Value) -> bool {
    let decision = &frame["public_base"]["decision"];
    let reward = &decision["card_reward"];
    let cards = match reward["cards"].as_array() {
        Some(cards) if cards.len() == 3 => cards,
        _ => return false,
    };
    let reward_indices = cards
        .iter()
        .filter_map(|card| card["reward_index"].as_u64())
        .collect::<BTreeSet<_>>();
    frame["boundary"]["kind"].as_str() == Some("run_decision")
        && frame["boundary"]["evidence"]["room_phase"].as_str() == Some("COMBAT")
        && decision["kind"].as_str() == Some("card_reward")
        && reward["source_kind"].as_str() == Some("combat_discovery")
        && reward["skip_available"].as_bool() == Some(false)
        && reward_indices == BTreeSet::from([0, 1, 2])
        && frame["public_base"]["combat"].is_object()
        && frame["public_base"]["combat"]["hand"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && frame["parity_internal"]["relics"]
            .as_array()
            .is_some_and(|relics| {
                relics
                    .iter()
                    .any(|relic| relic["relic_id"].as_str() == Some("Toolbox"))
            })
}

fn java_toolbox_overlay_actions_match(frame: &Value, actions: &[ActionHandle]) -> bool {
    if actions.iter().any(|action| {
        !matches!(
            action.kind.as_str(),
            "choose_card_reward" | "discard_potion"
        )
    }) {
        return false;
    }
    let cards = match frame["public_base"]["decision"]["card_reward"]["cards"].as_array() {
        Some(cards) if cards.len() == 3 => cards,
        _ => return false,
    };
    let indices = actions
        .iter()
        .filter_map(|action| {
            let index = action.reward_index?;
            let card = cards.get(index)?;
            (action.kind == "choose_card_reward"
                && action.card_id.as_deref() == card["card_id"].as_str()
                && action.card_upgrades == card["upgrades"].as_u64().map(|value| value as usize))
            .then_some(index)
        })
        .collect::<BTreeSet<_>>();
    indices == BTreeSet::from([0, 1, 2])
}

fn java_decision_matches_rust_site(view: &LiveStatus, rust_site: &str) -> bool {
    if rust_site == "terminal" {
        return java_frame_is_terminal_heart_victory(&view.frame);
    }
    if rust_site == "combat" {
        return view.frame["boundary"]["kind"].as_str() == Some("combat_player_decision");
    }
    if view.frame["boundary"]["kind"].as_str() != Some("run_decision") {
        return false;
    }
    let java_kind = view.frame["public_base"]["decision"]["kind"].as_str();
    match rust_site {
        "neow" | "event" => java_kind == Some("event"),
        "map" => java_kind == Some("map"),
        "reward" => java_kind == Some("combat_reward"),
        "card_reward" => java_kind == Some("card_reward"),
        "boss_relic" => java_kind == Some("boss_relic"),
        "shop" => java_kind == Some("shop"),
        "campfire" => java_kind == Some("smith_card"),
        "treasure" => java_kind == Some("treasure"),
        _ => false,
    }
}

fn java_frame_is_terminal_heart_victory(frame: &Value) -> bool {
    frame["boundary"]["kind"].as_str() == Some("terminal")
        && frame["public_base"]["decision"]["kind"].as_str() == Some("heart_victory")
        && frame["public_base"]["run"]["act"].as_i64() == Some(4)
        && frame["public_base"]["run"]["has_ruby_key"].as_bool() == Some(true)
        && frame["public_base"]["run"]["has_sapphire_key"].as_bool() == Some(true)
        && frame["public_base"]["run"]["has_emerald_key"].as_bool() == Some(true)
}

fn java_decision_matches_rust_step(view: &LiveStatus, step: &MirrorStep) -> bool {
    if step.site == "campfire" && matches!(step.action_kind.as_deref(), Some("rest" | "recall")) {
        return view.frame["boundary"]["kind"].as_str() == Some("run_decision")
            && view.frame["public_base"]["decision"]["kind"].as_str() == Some("campfire");
    }
    if step.site == "campfire" && step.action_kind.as_deref() == Some("smith") {
        return view.frame["boundary"]["kind"].as_str() == Some("run_decision")
            && view.frame["public_base"]["decision"]["kind"].as_str() == Some("smith_card");
    }
    if is_library_event_card_choice(step) {
        return view.frame["boundary"]["kind"].as_str() == Some("run_decision")
            && view.frame["public_base"]["decision"]["kind"].as_str() == Some("grid_select")
            && view.actions.iter().any(|action| {
                action.kind == "submit_grid_select"
                    && action.grid_select_source_pile.as_deref() == Some("EventOffer")
                    && action.grid_select_reason.as_deref() == Some("TheLibrary")
            });
    }
    if is_supported_event_deck_run_selection(step) {
        let Ok(selection_reason) = event_deck_grid_selection_reason(step) else {
            return false;
        };
        let Some(selection) = step.run_selection.as_ref() else {
            return false;
        };
        return view.frame["boundary"]["kind"].as_str() == Some("run_decision")
            && view.frame["public_base"]["decision"]["kind"].as_str() == Some("grid_select")
            && view.actions.iter().any(|action| {
                action.kind == "submit_grid_select"
                    && action.grid_select_source_pile.as_deref() == Some("MasterDeck")
                    && action.grid_select_reason.as_deref() == Some(selection_reason)
                    && action
                        .event_id
                        .as_deref()
                        .is_some_and(|java_id| event_ids_match(&selection.source_event, java_id))
            });
    }
    if step.action_kind.as_deref() == Some("submit_hand_select") {
        return view.frame["boundary"]["kind"].as_str() == Some("run_decision")
            && view.frame["public_base"]["decision"]["kind"].as_str() == Some("hand_select");
    }
    if step.action_kind.as_deref() == Some("submit_grid_select") {
        return view.frame["boundary"]["kind"].as_str() == Some("run_decision")
            && view.frame["public_base"]["decision"]["kind"].as_str() == Some("grid_select");
    }
    if step.action_kind.as_deref() == Some("submit_indexed_choice")
        && retained_combat_card_reward_source_kind(&step.selected_candidate).is_some()
    {
        let expected_source_kind =
            retained_combat_card_reward_source_kind(&step.selected_candidate);
        return view.frame["boundary"]["kind"].as_str() == Some("run_decision")
            && view.frame["public_base"]["decision"]["kind"].as_str() == Some("card_reward")
            && view.frame["public_base"]["decision"]["card_reward"]["source_kind"].as_str()
                == expected_source_kind;
    }
    java_decision_matches_rust_site(view, &step.site)
}

fn frame_id(view: &LiveStatus) -> Result<u64, String> {
    view.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "Java mirror frame omitted frame_id".to_owned())
}

fn file_identity(path: &Path) -> Result<MirrorFileIdentity, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    Ok(MirrorFileIdentity {
        path: absolute_string(path)?,
        bytes: metadata.len(),
        sha256: sha256_file(path)?,
    })
}

fn unix_ms() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{
        active_prefix_expected_successor_site, checkpoint_prefix_step_projection,
        checkpoint_semantic_projection, classify_expected_successor_combat_boundary,
        compare_combat_projection, current_combat_monster_ids, current_combat_successor_projection,
        current_run_profile_supports, current_run_profile_supports_selected,
        current_run_profile_supports_step, enemy_ids_match, event_deck_grid_selection_reason,
        event_ids_match, game_ids_match, java_frame_is_terminal_heart_victory,
        java_frame_matches_combat_boundary, java_seed_token_for_engine_seed,
        java_toolbox_overlay_actions_match, mirror_run_contract_error, mirror_step,
        ordered_enemy_ids_match, power_ids_match, project_candidate_to_captured_contract,
        resolve_java_target_from_frame, retained_action_kind,
        retained_combat_card_reward_source_kind, rust_head_is_initial_toolbox_overlay,
        selected_hand_card_uuids_from_candidates, validate_mirror_suffix_batch_member_count,
        verify_smith_postcondition, MirrorCombatProjection, MirrorStep,
        COMBAT_BOUNDARY_PLAYER_DECISION, COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD,
    };
    use crate::ActionHandle;
    use serde_json::{json, Value};

    #[test]
    fn engine_seed_uses_the_original_games_unsigned_base_35_display_encoding() {
        assert_eq!(java_seed_token_for_engine_seed("0").unwrap(), "0");
        assert_eq!(java_seed_token_for_engine_seed("34").unwrap(), "Z");
        assert_eq!(java_seed_token_for_engine_seed("35").unwrap(), "10");
        assert_eq!(
            java_seed_token_for_engine_seed("2026083101").unwrap(),
            "13K5KDL"
        );
        assert_eq!(
            java_seed_token_for_engine_seed("18446744073709551615").unwrap(),
            "5G24A25UXKXFF"
        );
        assert!(java_seed_token_for_engine_seed("13K5KDL").is_err());
    }

    #[test]
    fn suffix_batch_member_count_is_closed_before_file_validation() {
        assert!(validate_mirror_suffix_batch_member_count(1).is_err());
        validate_mirror_suffix_batch_member_count(2).unwrap();
        validate_mirror_suffix_batch_member_count(64).unwrap();
        assert!(validate_mirror_suffix_batch_member_count(65).is_err());
    }

    #[test]
    fn active_combat_endpoint_compiles_one_typed_successor_projection() {
        let head = json!({
            "current_decision": {
                "site": "combat",
                "observation": {
                    "domain": "combat",
                    "run_context": {"act": 2, "floor": 21},
                    "observation": {
                        "turn": {"turn_count": 2, "energy": 1},
                        "player": {
                            "hp": 31,
                            "max_hp": 80,
                            "block": 7,
                            "gold": 144,
                            "powers": [{"power": "Weak", "amount": 2}]
                        },
                        "cards": {"hand": {"cards": [{
                            "card_id": "Strike",
                            "upgrades": 1,
                            "effective_cost": 0
                        }]}},
                        "potions": [null, {"potion_id": "SwiftPotion"}],
                        "monsters": [
                            {
                                "slot": 0,
                                "enemy": {"status": "known", "enemy_id": "AcidSlimeS"},
                                "hp": 4,
                                "max_hp": 10,
                                "block": 0,
                                "alive": true,
                                "intent": {"intent": {"Attack": {"damage": 5, "hits": 1}}},
                                "powers": []
                            },
                            {
                                "slot": 1,
                                "enemy": {"status": "known", "enemy_id": "AcidSlimeS"},
                                "hp": 8,
                                "max_hp": 10,
                                "block": 3,
                                "alive": true,
                                "intent": {"intent": "Debuff"},
                                "powers": [{"power": "Vulnerable", "amount": 1}]
                            }
                        ]
                    }
                }
            }
        });

        let (scalars, combat) = current_combat_successor_projection(&head).unwrap();
        assert_eq!(scalars.act, 2);
        assert_eq!(scalars.floor, 21);
        assert_eq!(scalars.current_hp, 31);
        assert_eq!(combat.turn_index, 2);
        assert_eq!(combat.energy, 1);
        assert_eq!(combat.player_block, 7);
        assert_eq!(combat.hand[0].card_id, "Strike");
        assert_eq!(combat.hand[0].cost_for_turn, 0);
        assert_eq!(combat.potions, vec![None, Some("SwiftPotion".to_owned())]);
        assert_eq!(combat.player_powers[0].power_id, "Weak");
        assert_eq!(combat.monsters[0].same_enemy_ordinal, 0);
        assert_eq!(combat.monsters[0].intent.as_deref(), Some("attack"));
        assert_eq!(combat.monsters[1].same_enemy_ordinal, 1);
        assert_eq!(combat.monsters[1].intent.as_deref(), Some("debuff"));
        assert_eq!(combat.monsters[1].powers[0].power_id, "Vulnerable");
    }

    fn toolbox_rust_head() -> Value {
        let candidate = |choice_index, card_id| {
            json!({
                "candidate": {
                    "domain": "combat_indexed_choice",
                    "action": {"kind": "submit_indexed_choice", "choice_index": choice_index},
                    "input_encoding": "submit_discover_choice_index",
                    "reason": {"kind": "card_reward", "destination": "Hand"},
                    "candidate": {"kind": "card", "card_id": card_id, "upgrades": 0}
                }
            })
        };
        json!({
            "current_decision": {
                "site": "combat",
                "observation": {"observation": {
                    "turn": {
                        "turn_count": 0,
                        "counters": {"cards_played_this_turn": 0}
                    },
                    "cards": {"hand": {"cards": []}},
                    "player": {"relics": [{"id": "Toolbox"}]}
                }},
                "candidates": [
                    candidate(0, "TheBomb"),
                    candidate(1, "Purity"),
                    candidate(2, "Trip")
                ]
            }
        })
    }

    fn toolbox_java_frame() -> Value {
        json!({
            "boundary": {
                "kind": "run_decision",
                "evidence": {"room_phase": "COMBAT"}
            },
            "public_base": {
                "decision": {
                    "kind": "card_reward",
                    "card_reward": {
                        "source_kind": "combat_discovery",
                        "cards": [
                            {"reward_index": 0, "card_id": "The Bomb", "upgrades": 0},
                            {"reward_index": 1, "card_id": "Purity", "upgrades": 0},
                            {"reward_index": 2, "card_id": "Trip", "upgrades": 0}
                        ],
                        "skip_available": false
                    }
                },
                "combat": {"hand": [], "monsters": [{"id": "GiantHead"}]}
            },
            "parity_internal": {"relics": [{"relic_id": "Toolbox"}]}
        })
    }

    fn card_reward_action(reward_index: usize, card_id: &str) -> ActionHandle {
        ActionHandle {
            handle: format!("h{reward_index}"),
            action_id: format!("a{reward_index}"),
            kind: "choose_card_reward".to_owned(),
            card_uuid: None,
            card_id: Some(card_id.to_owned()),
            card_upgrades: Some(0),
            card_ordinal: None,
            hand_index: None,
            potion_slot: None,
            potion_id: None,
            potion_potency: None,
            target_monster_id: None,
            target_monster_instance_id: None,
            target_monster_index: None,
            event_id: None,
            option_index: None,
            option_text: None,
            neow_reward_type: None,
            neow_drawback: None,
            map_x: None,
            map_y: None,
            map_room_type: None,
            reward_type: Some("CARD".to_owned()),
            reward_index: Some(reward_index),
            reward_id: None,
            reward_gold: None,
            shop_slot: None,
            shop_price: None,
            ftue_id: None,
            hand_select_id: None,
            hand_select_allowed_card_uuids: Vec::new(),
            hand_select_allowed_counts: Vec::new(),
            grid_select_id: None,
            grid_select_source_pile: None,
            grid_select_reason: None,
            grid_select_allowed_card_uuids: Vec::new(),
            grid_select_allowed_counts: Vec::new(),
            parameters_required: false,
        }
    }

    #[test]
    fn combat_endpoint_classifies_only_the_closed_initial_toolbox_shape() {
        let head = toolbox_rust_head();
        assert!(rust_head_is_initial_toolbox_overlay(&head));
        assert_eq!(
            classify_expected_successor_combat_boundary(&head, "combat").unwrap(),
            Some(COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD.to_owned())
        );
        assert_eq!(
            classify_expected_successor_combat_boundary(&head, "map").unwrap(),
            None
        );

        let mut nonempty_hand = head.clone();
        nonempty_hand["current_decision"]["observation"]["observation"]["cards"]["hand"]["cards"] =
            json!([{"card_id": "Strike"}]);
        assert!(!rust_head_is_initial_toolbox_overlay(&nonempty_hand));
        assert!(classify_expected_successor_combat_boundary(&nonempty_hand, "combat").is_err());

        let mut missing_toolbox = head;
        missing_toolbox["current_decision"]["observation"]["observation"]["player"]["relics"] =
            json!([]);
        assert!(!rust_head_is_initial_toolbox_overlay(&missing_toolbox));
        assert!(classify_expected_successor_combat_boundary(&missing_toolbox, "combat").is_err());
    }

    #[test]
    fn java_toolbox_boundary_rejects_reward_and_combat_shape_aliases() {
        let frame = toolbox_java_frame();
        assert!(java_frame_matches_combat_boundary(
            &frame,
            COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD
        ));
        assert!(!java_frame_matches_combat_boundary(
            &frame,
            COMBAT_BOUNDARY_PLAYER_DECISION
        ));

        let mut ordinary_reward = frame.clone();
        ordinary_reward["public_base"]["decision"]["card_reward"]["source_kind"] =
            json!("ordinary_reward");
        assert!(!java_frame_matches_combat_boundary(
            &ordinary_reward,
            COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD
        ));

        let mut skippable = frame.clone();
        skippable["public_base"]["decision"]["card_reward"]["skip_available"] = json!(true);
        assert!(!java_frame_matches_combat_boundary(
            &skippable,
            COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD
        ));

        let mut nonempty_hand = frame.clone();
        nonempty_hand["public_base"]["combat"]["hand"] = json!([{"card_id": "Strike"}]);
        assert!(!java_frame_matches_combat_boundary(
            &nonempty_hand,
            COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD
        ));

        let mut missing_toolbox = frame.clone();
        missing_toolbox["parity_internal"]["relics"] = json!([]);
        assert!(!java_frame_matches_combat_boundary(
            &missing_toolbox,
            COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD
        ));

        let mut noncombat = frame;
        noncombat["boundary"]["evidence"]["room_phase"] = json!("COMPLETE");
        assert!(!java_frame_matches_combat_boundary(
            &noncombat,
            COMBAT_BOUNDARY_TOOLBOX_CARD_REWARD
        ));
    }

    #[test]
    fn java_toolbox_boundary_requires_three_exact_bound_choose_actions() {
        let frame = toolbox_java_frame();
        let mut actions = vec![
            card_reward_action(0, "The Bomb"),
            card_reward_action(1, "Purity"),
            card_reward_action(2, "Trip"),
        ];
        assert!(java_toolbox_overlay_actions_match(&frame, &actions));
        assert!(!java_toolbox_overlay_actions_match(&frame, &actions[..2]));

        let mut discard = card_reward_action(0, "The Bomb");
        discard.kind = "discard_potion".to_owned();
        discard.reward_index = None;
        discard.card_id = None;
        discard.card_upgrades = None;
        discard.potion_slot = Some(0);
        discard.potion_id = Some("EntropicBrew".to_owned());
        actions.push(discard);
        assert!(java_toolbox_overlay_actions_match(&frame, &actions));

        let mut wrong_card = actions.clone();
        wrong_card[2].card_id = Some("Panacea".to_owned());
        assert!(!java_toolbox_overlay_actions_match(&frame, &wrong_card));

        let mut unrelated = actions;
        unrelated[3].kind = "skip_card_reward".to_owned();
        assert!(!java_toolbox_overlay_actions_match(&frame, &unrelated));
    }

    #[test]
    fn heart_mirror_rejects_missing_or_disabled_final_act_at_the_first_frame() {
        assert!(mirror_run_contract_error(
            "heart_victory",
            &json!({"public_base": {"run": {"final_act_available": true}}})
        )
        .is_none());
        assert!(mirror_run_contract_error(
            "heart_victory",
            &json!({"public_base": {"run": {"final_act_available": false}}})
        )
        .unwrap()
        .contains("reported false"));
        assert!(
            mirror_run_contract_error("heart_victory", &json!({"public_base": {"run": {}}}))
                .unwrap()
                .contains("omitted")
        );
    }

    #[test]
    fn active_prefix_accepts_only_an_exact_heart_victory_without_a_current_decision() {
        assert_eq!(
            active_prefix_expected_successor_site(None, true).unwrap(),
            "terminal"
        );
        assert_eq!(
            active_prefix_expected_successor_site(Some("combat"), false).unwrap(),
            "combat"
        );
        assert!(active_prefix_expected_successor_site(None, false).is_err());
    }

    #[test]
    fn terminal_scope_requires_the_closed_act_four_three_key_victory_shape() {
        let frame = json!({
            "boundary": {"kind": "terminal"},
            "public_base": {
                "run": {
                    "act": 4,
                    "has_ruby_key": true,
                    "has_sapphire_key": true,
                    "has_emerald_key": true
                },
                "decision": {"kind": "heart_victory"}
            }
        });
        assert!(java_frame_is_terminal_heart_victory(&frame));

        let mut death = frame.clone();
        death["public_base"]["decision"]["kind"] = json!("death");
        assert!(!java_frame_is_terminal_heart_victory(&death));

        let mut missing_key = frame.clone();
        missing_key["public_base"]["run"]["has_emerald_key"] = json!(false);
        assert!(!java_frame_is_terminal_heart_victory(&missing_key));

        let mut unsupported = frame;
        unsupported["boundary"]["kind"] = json!("unsupported_run_decision");
        assert!(!java_frame_is_terminal_heart_victory(&unsupported));
    }

    #[test]
    fn display_spaced_java_ids_match_compact_rust_ids() {
        assert!(game_ids_match("WhiteBeastStatue", "White Beast Statue"));
        assert!(game_ids_match("SwiftPotion", "Swift Potion"));
        assert!(game_ids_match("WeakenPotion", "Weak Potion"));
        assert!(game_ids_match("DistilledChaosPotion", "Distilled Chaos"));
        assert!(game_ids_match("Elixir", "ElixirPotion"));
        assert!(!game_ids_match("WhiteBeastStatue", "Mercury Hourglass"));
    }

    #[test]
    fn power_ids_match_only_explicit_original_game_aliases() {
        assert!(power_ids_match("Weak", "Weakened"));
        assert!(power_ids_match("Weakened", "Weak"));
        assert!(power_ids_match("Regrow", "Life Link"));
        assert!(power_ids_match("Life Link", "Regrow"));
        assert!(power_ids_match("Regen", "Regenerate"));
        assert!(power_ids_match("Regenerate", "Regen"));
        assert!(!power_ids_match("Weak", "Vulnerable"));
        assert!(!power_ids_match("Weakness", "Weakened"));
        assert!(!power_ids_match("Regrow", "Regeneration"));
        assert!(!power_ids_match("Regen", "Regeneration"));
    }

    #[test]
    fn enemy_ids_match_only_explicit_original_game_aliases() {
        assert!(enemy_ids_match("BanditPointy", "BanditChild"));
        assert!(enemy_ids_match("BanditChild", "BanditPointy"));
        assert!(enemy_ids_match("LouseDefensive", "FuzzyLouseDefensive"));
        assert!(enemy_ids_match("LouseNormal", "FuzzyLouseNormal"));
        assert!(enemy_ids_match("SpireGrowth", "Serpent"));
        assert!(enemy_ids_match("Serpent", "SpireGrowth"));
        assert!(!enemy_ids_match("BanditPointy", "BanditLeader"));
        assert!(!enemy_ids_match("LouseDefensive", "FuzzyLouseNormal"));
        assert!(!enemy_ids_match("Serpent", "TheMaw"));
        assert!(game_ids_match("BanditPointy", "BanditChild"));
        assert!(game_ids_match("SpireGrowth", "Serpent"));
    }

    #[test]
    fn combat_target_identity_maps_duplicate_enemy_occurrence_not_raw_group_index() {
        let combat: MirrorCombatProjection = serde_json::from_value(json!({
            "turn_index": 1,
            "energy": 2,
            "player_block": 25,
            "hand": [],
            "potions": [],
            "player_powers": [],
            "monsters": [
                {
                    "source_index": 0,
                    "source_slot": 0,
                    "enemy_id": "TheCollector",
                    "same_enemy_ordinal": 0,
                    "hp": 273,
                    "max_hp": 300,
                    "block": 0,
                    "alive": true,
                    "intent": "defend_buff",
                    "powers": []
                },
                {
                    "source_index": 1,
                    "source_slot": 1,
                    "enemy_id": "TorchHead",
                    "same_enemy_ordinal": 0,
                    "hp": 24,
                    "max_hp": 41,
                    "block": 0,
                    "alive": true,
                    "intent": "attack",
                    "powers": []
                },
                {
                    "source_index": 2,
                    "source_slot": 2,
                    "enemy_id": "TorchHead",
                    "same_enemy_ordinal": 1,
                    "hp": 30,
                    "max_hp": 40,
                    "block": 0,
                    "alive": true,
                    "intent": "attack",
                    "powers": []
                }
            ]
        }))
        .unwrap();
        let frame = json!({
            "public_base": {"combat": {"monsters": [
                {"index": 0, "instance_id": 7, "id": "TorchHead"},
                {"index": 1, "instance_id": 8, "id": "TorchHead"},
                {"index": 2, "instance_id": 6, "id": "TheCollector"}
            ]}}
        });

        let target = resolve_java_target_from_frame(&frame, &combat, 2)
            .unwrap()
            .unwrap();
        assert_eq!(target.index, 1);
        assert_eq!(target.instance_id, 8);
        assert_eq!(target.monster_id, "TorchHead");
    }

    #[test]
    fn per_action_combat_projection_reports_the_first_monster_order_divergence() {
        let expected: MirrorCombatProjection = serde_json::from_value(json!({
            "turn_index": 1,
            "energy": 2,
            "player_block": 25,
            "hand": [],
            "potions": [],
            "player_powers": [],
            "monsters": [
                {
                    "source_index": 0,
                    "source_slot": 0,
                    "enemy_id": "TheCollector",
                    "same_enemy_ordinal": 0,
                    "hp": 273,
                    "max_hp": 300,
                    "block": 0,
                    "alive": true,
                    "intent": "defend_buff",
                    "powers": []
                },
                {
                    "source_index": 1,
                    "source_slot": 1,
                    "enemy_id": "TorchHead",
                    "same_enemy_ordinal": 0,
                    "hp": 24,
                    "max_hp": 41,
                    "block": 0,
                    "alive": true,
                    "intent": "attack",
                    "powers": []
                }
            ]
        }))
        .unwrap();
        let java = json!({
            "public_base": {"combat": {
                "turn": 2,
                "player": {"energy": 2, "block": 25},
                "hand": [],
                "potions": [],
                "monsters": [
                    {"index": 0, "instance_id": 7, "id": "TorchHead", "current_hp": 24, "max_hp": 41, "block": 0, "dead_or_escaped": false},
                    {"index": 1, "instance_id": 6, "id": "TheCollector", "current_hp": 273, "max_hp": 300, "block": 0, "dead_or_escaped": false}
                ]
            }},
            "parity_internal": {
                "player_powers": [],
                "monsters": [
                    {"intent": "ATTACK", "powers": []},
                    {"intent": "DEFEND_BUFF", "powers": []}
                ]
            }
        });

        let mismatch = compare_combat_projection(&java, &expected).unwrap();
        assert!(mismatch.contains("monster[0]"));
        assert!(mismatch.contains("TheCollector"));
        assert!(mismatch.contains("TorchHead"));
    }

    #[test]
    fn per_action_combat_projection_ignores_render_lifetime_fields_after_death() {
        let expected: MirrorCombatProjection = serde_json::from_value(json!({
            "turn_index": 1,
            "energy": 2,
            "player_block": 25,
            "hand": [],
            "potions": [],
            "player_powers": [],
            "monsters": [{
                "source_index": 0,
                "source_slot": 0,
                "enemy_id": "Deca",
                "same_enemy_ordinal": 0,
                "hp": 0,
                "max_hp": 265,
                "block": 0,
                "alive": false,
                "intent": "defend_buff",
                "powers": []
            }]
        }))
        .unwrap();
        let java = json!({
            "public_base": {"combat": {
                "turn": 2,
                "player": {"energy": 2, "block": 25},
                "hand": [],
                "potions": [],
                "monsters": [{
                    "index": 0,
                    "instance_id": 1,
                    "id": "Deca",
                    "current_hp": 0,
                    "max_hp": 265,
                    "block": 0,
                    "dead_or_escaped": true
                }]
            }},
            "parity_internal": {
                "player_powers": [],
                "monsters": [{
                    "intent": "DEFEND_BUFF",
                    "powers": [{"id": "Plated Armor", "amount": 1}]
                }]
            }
        });

        assert_eq!(compare_combat_projection(&java, &expected), None);
    }

    #[test]
    fn run_profile_supports_typed_noncombat_potion_discard() {
        assert!(current_run_profile_supports(
            "reward",
            Some("discard_run_potion"),
            "strategic"
        ));
        assert!(!current_run_profile_supports(
            "combat",
            Some("discard_run_potion"),
            "combat_atomic"
        ));
        assert!(current_run_profile_supports(
            "boss_relic",
            Some("take_boss_relic"),
            "strategic"
        ));
        assert!(current_run_profile_supports(
            "boss_relic",
            Some("skip_boss_relic"),
            "strategic"
        ));
        assert!(current_run_profile_supports(
            "shop",
            Some("buy_potion"),
            "strategic"
        ));
    }

    #[test]
    fn run_profile_supports_reward_cancel_only_at_the_typed_reward_boundary() {
        assert!(current_run_profile_supports(
            "reward",
            Some("cancel"),
            "strategic"
        ));
        assert!(!current_run_profile_supports(
            "event",
            Some("cancel"),
            "strategic"
        ));
        assert!(!current_run_profile_supports(
            "reward",
            Some("cancel"),
            "combat_atomic"
        ));
    }

    #[test]
    fn checkpoint_prefix_ignores_ui_order_for_self_identifying_typed_actions() {
        let source = json!({
            "depth": 440,
            "site": "reward",
            "before": {"act": 2, "floor": 31, "current_hp": 39, "max_hp": 82, "gold": 19},
            "semantic_action": "proceed",
            "selected_candidate": {
                "domain": "strategic",
                "action": {"kind": "proceed", "site": "reward"}
            },
            "choice": {"ordinal_path": [2]}
        });
        let mut reordered = source.clone();
        reordered["choice"]["ordinal_path"] = json!([3]);

        let source = mirror_step(&source, None).unwrap();
        let reordered = mirror_step(&reordered, None).unwrap();
        assert_eq!(
            checkpoint_prefix_step_projection(&source),
            checkpoint_prefix_step_projection(&reordered)
        );

        let mut changed_action = source.clone();
        changed_action.selected_candidate["action"]["kind"] = json!("cancel");
        assert_ne!(
            checkpoint_prefix_step_projection(&source),
            checkpoint_prefix_step_projection(&changed_action)
        );
    }

    #[test]
    fn run_profile_supports_only_typed_shop_card_removal() {
        assert!(current_run_profile_supports(
            "shop",
            Some("remove_card"),
            "strategic"
        ));
        assert!(!current_run_profile_supports(
            "shop",
            Some("remove_card"),
            "combat_atomic"
        ));
        assert!(!current_run_profile_supports(
            "event",
            Some("remove_card"),
            "strategic"
        ));
    }

    #[test]
    fn run_profile_supports_typed_campfire_actions_only_in_their_strategic_site() {
        assert!(current_run_profile_supports(
            "campfire",
            Some("rest"),
            "strategic"
        ));
        assert!(current_run_profile_supports(
            "campfire",
            Some("recall"),
            "strategic"
        ));
        assert!(!current_run_profile_supports(
            "campfire",
            Some("rest"),
            "combat_atomic"
        ));
        assert!(!current_run_profile_supports(
            "event",
            Some("recall"),
            "strategic"
        ));
    }

    #[test]
    fn run_profile_supports_one_typed_bonfire_deck_selection() {
        let previous = json!({
            "depth": 10,
            "site": "event",
            "before": {"act": 3, "floor": 45, "current_hp": 73, "max_hp": 82, "gold": 130},
            "choice": {"ordinal_path": [3]},
            "semantic_action": "event:BonfireElementals:0",
            "selected_candidate": {"domain": "strategic", "action": {
                "kind": "choose_event_option",
                "event": "BonfireElementals",
                "screen": 1,
                "public_semantics": {"structured_semantics": {"transition": {
                    "kind": "open_selection", "selection": "offer_card"
                }}}
            }}
        });
        let selected = json!({
            "depth": 11,
            "site": "run_choice",
            "before": {"act": 3, "floor": 45, "current_hp": 73, "max_hp": 82, "gold": 130},
            "choice": {"ordinal_path": [45]},
            "semantic_action": "select:Normality+0#0",
            "selected_candidate": {"domain": "strategic", "action": {
                "kind": "submit_run_selection",
                "scope": "Deck",
                "selected_card_ordinals": [43]
            }}
        });
        let step = mirror_step(&selected, Some(&previous)).unwrap();
        let selection = step.run_selection.as_ref().unwrap();
        assert_eq!(selection.card_id, "Normality");
        assert_eq!(selection.selected_card_ordinal, 43);
        assert!(current_run_profile_supports_step(&step));

        let mut duplicate = selected;
        duplicate["semantic_action"] = json!("select:Normality+0#1");
        let duplicate = mirror_step(&duplicate, Some(&previous)).unwrap();
        assert!(current_run_profile_supports_step(&duplicate));
    }

    #[test]
    fn run_profile_supports_typed_living_wall_deck_mutations() {
        for (selection_kind, expected_reason) in [
            ("remove_card", "DeckRemove"),
            ("transform_card", "DeckTransform"),
            ("upgrade_card", "DeckUpgrade"),
        ] {
            let previous = json!({
                "depth": 66,
                "site": "event",
                "before": {"act": 1, "floor": 5, "current_hp": 75, "max_hp": 75, "gold": 22},
                "choice": {"ordinal_path": [2]},
                "semantic_action": "event:LivingWall:1",
                "selected_candidate": {"domain": "strategic", "action": {
                    "kind": "choose_event_option",
                    "event": "LivingWall",
                    "screen": 0,
                    "public_semantics": {"structured_semantics": {"transition": {
                        "kind": "open_selection", "selection": selection_kind
                    }}}
                }}
            });
            let selected = json!({
                "depth": 67,
                "site": "run_choice",
                "before": {"act": 1, "floor": 5, "current_hp": 75, "max_hp": 75, "gold": 22},
                "choice": {"ordinal_path": [1]},
                "semantic_action": "select:Strike+0#0",
                "selected_candidate": {"domain": "strategic", "action": {
                    "kind": "submit_run_selection",
                    "scope": "Deck",
                    "selected_card_ordinals": [1]
                }}
            });
            let step = mirror_step(&selected, Some(&previous)).unwrap();
            let selection = step.run_selection.as_ref().unwrap();
            assert_eq!(selection.source_event, "LivingWall");
            assert_eq!(selection.selection_kind.as_deref(), Some(selection_kind));
            assert_eq!(selection.card_id, "Strike");
            assert_eq!(
                event_deck_grid_selection_reason(&step).unwrap(),
                expected_reason
            );
            assert!(current_run_profile_supports_step(&step));
        }
    }

    #[test]
    fn event_deck_mutation_support_is_operation_typed_not_event_whitelisted() {
        let previous = json!({
            "depth": 80,
            "site": "event",
            "before": {"act": 2, "floor": 30, "current_hp": 50, "max_hp": 75, "gold": 10},
            "choice": {"ordinal_path": [1]},
            "semantic_action": "event:FutureDeckEvent:0",
            "selected_candidate": {"domain": "strategic", "action": {
                "kind": "choose_event_option",
                "event": "FutureDeckEvent",
                "screen": 7,
                "public_semantics": {"structured_semantics": {"transition": {
                    "kind": "open_selection", "selection": "transform_card"
                }}}
            }}
        });
        let selected = json!({
            "depth": 81,
            "site": "run_choice",
            "before": {"act": 2, "floor": 30, "current_hp": 50, "max_hp": 75, "gold": 10},
            "choice": {"ordinal_path": [1]},
            "semantic_action": "select:Strike+0#0",
            "selected_candidate": {"domain": "strategic", "action": {
                "kind": "submit_run_selection",
                "scope": "Deck",
                "selected_card_ordinals": [3]
            }}
        });
        let step = mirror_step(&selected, Some(&previous)).unwrap();
        assert_eq!(
            event_deck_grid_selection_reason(&step).unwrap(),
            "DeckTransform"
        );
        assert!(current_run_profile_supports_step(&step));
    }

    #[test]
    fn ironclad_starter_ids_match_java_color_suffixes() {
        assert!(game_ids_match("Strike", "Strike_R"));
        assert!(game_ids_match("Defend", "Defend_R"));
        assert!(!game_ids_match("Strike", "Defend_R"));
    }

    #[test]
    fn relic_ids_match_original_game_external_aliases() {
        for (stable, external) in [
            ("Abacus", "TheAbacus"),
            ("Courier", "The Courier"),
            ("Duality", "Yang"),
            ("GoldPlatedCables", "Cables"),
            ("NeowsLament", "NeowsBlessing"),
            ("PhilosopherStone", "Philosopher's Stone"),
            ("SnakeRing", "Ring of the Snake"),
            ("SneckoSkull", "Snake Skull"),
            ("Waffle", "Lee's Waffle"),
            ("WingBoots", "WingedGreaves"),
            ("ToxicEgg", "Toxic Egg 2"),
            ("FrozenEgg", "Frozen Egg 2"),
            ("MoltenEgg", "Molten Egg 2"),
        ] {
            assert!(game_ids_match(stable, external), "{stable} != {external}");
        }
        assert!(!game_ids_match("FrozenEgg", "Toxic Egg 2"));
    }

    #[test]
    fn active_prefix_combat_binds_ordered_monster_ids() {
        let head = json!({
            "current_decision": {"observation": {"observation": {"monsters": [
                {"enemy": {"status": "known", "enemy_id": "TheGuardian"}}
            ]}}}
        });
        let expected = current_combat_monster_ids(&head).unwrap();
        assert!(ordered_enemy_ids_match(
            &expected,
            &["The Guardian".to_owned()]
        ));
        assert!(!ordered_enemy_ids_match(
            &expected,
            &["Hexaghost".to_owned()]
        ));
    }

    #[test]
    fn retained_hand_selection_family_has_one_typed_mirror_action_kind() {
        let step = json!({
            "selected_candidate": {
                "domain": "combat_selection_family",
                "family": {
                    "input_encoding": "submit_selection_hand_card_uuids"
                }
            }
        });
        assert_eq!(
            retained_action_kind(&step).as_deref(),
            Some("submit_hand_select")
        );
    }

    #[test]
    fn hand_selection_preserves_every_selected_occurrence_after_the_root_ordinal() {
        let step: MirrorStep = serde_json::from_value(json!({
            "depth": 17,
            "site": "combat",
            "before": {"act": 4, "floor": 55, "current_hp": 82, "max_hp": 82, "gold": 1},
            "semantic_action": "select:Burn+0#0,Burn+0#1,AscendersBane+0#0",
            "candidate_domain": "combat_selection_family",
            "action_kind": "submit_hand_select",
            "selected_candidate": {"family": {"domain": [
                {"kind": "card", "eligible": true, "ordinal": 0, "card_id": "BattleTrance", "upgrades": 1},
                {"kind": "card", "eligible": true, "ordinal": 2, "card_id": "Burn", "upgrades": 0},
                {"kind": "card", "eligible": true, "ordinal": 3, "card_id": "Burn", "upgrades": 0},
                {"kind": "card", "eligible": true, "ordinal": 5, "card_id": "AscendersBane", "upgrades": 0}
            ]}},
            "choice": {"ordinal_path": [9, 1, 2, 3]}
        }))
        .unwrap();
        let candidates = json!([
            {"index": 0, "card_uuid": "trance", "card_id": "Battle Trance", "upgrades": 1},
            {"index": 2, "card_uuid": "burn-a", "card_id": "Burn", "upgrades": 0},
            {"index": 3, "card_uuid": "burn-b", "card_id": "Burn", "upgrades": 0},
            {"index": 5, "card_uuid": "bane", "card_id": "AscendersBane", "upgrades": 0}
        ]);

        assert_eq!(
            selected_hand_card_uuids_from_candidates(candidates.as_array().unwrap(), &step)
                .unwrap(),
            ["burn-a", "burn-b", "bane"]
        );

        let mut empty = step;
        empty.choice = json!({"ordinal_path": [9]});
        assert!(
            selected_hand_card_uuids_from_candidates(candidates.as_array().unwrap(), &empty)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn retained_grid_selection_family_has_one_typed_mirror_action_kind() {
        let step = json!({
            "selected_candidate": {
                "domain": "combat_selection_family",
                "family": {
                    "input_encoding": "submit_selection_grid_card_uuids"
                }
            }
        });
        assert_eq!(
            retained_action_kind(&step).as_deref(),
            Some("submit_grid_select")
        );
        assert!(current_run_profile_supports(
            "combat",
            Some("submit_grid_select"),
            "combat_selection_family"
        ));
    }

    #[test]
    fn run_profile_supports_only_closed_combat_card_reward_indexed_choices() {
        let discovery = json!({
            "domain": "combat_indexed_choice",
            "action": {
                "kind": "submit_indexed_choice",
                "choice_index": 2
            },
            "input_encoding": "submit_discover_choice_index",
            "reason": {
                "kind": "discovery",
                "colorless": false,
                "card_type": "Power",
                "amount": 1
            },
            "candidate": {
                "kind": "card",
                "card_id": "Barricade",
                "upgrades": 0
            }
        });
        assert_eq!(
            retained_combat_card_reward_source_kind(&discovery),
            Some("combat_discovery")
        );
        assert!(current_run_profile_supports_selected(
            "combat",
            Some("submit_indexed_choice"),
            "combat_indexed_choice",
            &discovery
        ));

        let mut choose_one = discovery.clone();
        choose_one["reason"] = json!({
            "kind": "card_reward",
            "destination": "Hand"
        });
        choose_one["candidate"] = json!({
            "kind": "card",
            "card_id": "MasterOfStrategy",
            "upgrades": 0
        });
        assert_eq!(
            retained_combat_card_reward_source_kind(&choose_one),
            Some("combat_discovery")
        );
        assert!(current_run_profile_supports_selected(
            "combat",
            Some("submit_indexed_choice"),
            "combat_indexed_choice",
            &choose_one
        ));

        let mut wrong_destination = choose_one.clone();
        wrong_destination["reason"]["destination"] = json!("Draw");
        assert_eq!(
            retained_combat_card_reward_source_kind(&wrong_destination),
            None
        );
        assert!(!current_run_profile_supports_selected(
            "combat",
            Some("submit_indexed_choice"),
            "combat_indexed_choice",
            &wrong_destination
        ));

        let mut unrelated = discovery.clone();
        unrelated["input_encoding"] = json!("submit_arbitrary_index");
        assert_eq!(retained_combat_card_reward_source_kind(&unrelated), None);
        assert!(!current_run_profile_supports_selected(
            "combat",
            Some("submit_indexed_choice"),
            "combat_indexed_choice",
            &unrelated
        ));
    }

    #[test]
    fn checkpoint_projection_drops_only_dead_map_rng_after_the_emerald_key() {
        let frame = |has_emerald_key| {
            json!({
                "public_base": {
                    "run": {"has_emerald_key": has_emerald_key}
                },
                "parity_internal": {},
                "rng_internal": {
                    "run_streams": [
                        {
                            "owner": "com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng",
                            "counter": 94
                        },
                        {
                            "owner": "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRng",
                            "counter": 353
                        }
                    ]
                }
            })
        };

        let after_key = checkpoint_semantic_projection(&frame(true));
        let after_key_streams = after_key["rng_internal"]["run_streams"].as_array().unwrap();
        assert_eq!(after_key_streams.len(), 1);
        assert!(after_key_streams[0]["owner"]
            .as_str()
            .unwrap()
            .ends_with("cardRng"));

        let before_key = checkpoint_semantic_projection(&frame(false));
        assert_eq!(
            before_key["rng_internal"]["run_streams"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn checkpoint_projection_ignores_only_recreated_card_runtime_identity() {
        let frame = |card_uuid, instance_uuid, offered_card| {
            json!({
                "public_base": {
                    "run": {"has_emerald_key": true},
                    "decision": {"card_reward": {"cards": [{
                        "card_uuid": card_uuid,
                        "card_id": offered_card,
                        "upgrades": 0
                    }]}}
                },
                "parity_internal": {
                    "master_deck": [{
                        "card_uuid": card_uuid,
                        "card_id": "Strike_R",
                        "upgrades": 0
                    }],
                    "zones": {"draw": {"cards": [{
                        "instance_uuid": instance_uuid,
                        "card_id": "Strike_R",
                        "upgrades": 0
                    }]}}
                },
                "rng_internal": {"run_streams": []}
            })
        };
        let left = checkpoint_semantic_projection(&frame("card-a", "instance-a", "The Bomb"));
        let right = checkpoint_semantic_projection(&frame("card-b", "instance-b", "The Bomb"));
        assert_eq!(left, right);

        let changed_card = checkpoint_semantic_projection(&frame("card-c", "instance-c", "Purity"));
        assert_ne!(left, changed_card);
    }

    #[test]
    fn checkpoint_contract_accepts_new_observation_fields_but_not_changed_captured_state() {
        let captured = json!({
            "public_base": {"run": {"floor": 5}},
            "parity_internal": {"zones": {"hand": []}},
            "rng_internal": {"run_streams": []}
        });
        let mut resumed = captured.clone();
        resumed["parity_internal"]["master_deck"] = json!([]);
        resumed["parity_internal"]["card_pools"] = json!({"common": []});

        let captured_projection = checkpoint_semantic_projection(&captured);
        let resumed_projection = checkpoint_semantic_projection(&resumed);
        assert_eq!(
            captured_projection,
            project_candidate_to_captured_contract(&captured_projection, &resumed_projection)
        );

        resumed["public_base"]["run"]["floor"] = json!(6);
        let changed_projection = checkpoint_semantic_projection(&resumed);
        assert_ne!(
            captured_projection,
            project_candidate_to_captured_contract(&captured_projection, &changed_projection)
        );
    }

    #[test]
    fn smith_postcondition_requires_the_bound_occurrence_to_gain_one_upgrade() {
        let action = json!({
            "card_ordinal": 12,
            "card": "BattleTrance",
            "upgrades": 0
        });
        let upgraded = json!({
            "parity_internal": {
                "master_deck": [{
                    "ordinal": 12,
                    "card_id": "Battle Trance",
                    "upgrades": 1
                }]
            }
        });
        let unchanged = json!({
            "parity_internal": {
                "master_deck": [{
                    "ordinal": 12,
                    "card_id": "Battle Trance",
                    "upgrades": 0
                }]
            }
        });

        assert!(verify_smith_postcondition(&upgraded, &action).is_ok());
        assert!(verify_smith_postcondition(&unchanged, &action)
            .unwrap_err()
            .contains("before=0 after=0"));
    }

    #[test]
    fn event_identity_requires_the_same_stable_id() {
        assert!(event_ids_match("ScrapOoze", "ScrapOoze"));
        assert!(event_ids_match("GremlinWheelGame", "GremlinWheelGame"));
        assert!(event_ids_match("Neow", "Neow"));
        assert!(event_ids_match("BonfireElementals", "BonfireElementals"));
        assert!(event_ids_match("WorldOfGoop", "WorldOfGoop"));
        assert!(!event_ids_match("WorldOfGoop", "ScrapOoze"));
        assert!(!event_ids_match("GremlinWheelGame", "WeMeetAgain"));
        assert!(!event_ids_match(
            "ScrapOoze",
            "com.megacrit.cardcrawl.events.exordium.ScrapOoze"
        ));
    }
}
