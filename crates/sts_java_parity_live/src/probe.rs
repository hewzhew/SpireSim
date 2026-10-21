use super::{
    empty_object, publish_json_create_new, receipt_path, session_paths_from_manifest, sha256_file,
    start_run, status, submit_action, submit_hand_select, unix_ms, ActionHandle, ActionReceipt,
    LiveStatus, SessionManifest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const RECIPE_ID: &str = "elixir_hand_select";
const RECIPE_VERSION: u32 = 1;
const NATURAL_STRIKE_RECIPE_ID: &str = "seed_0_a20_ironclad_first_combat_base_strike";
const NATURAL_STRIKE_RECIPE_VERSION: u32 = 1;
const MAX_FTUE_ACKNOWLEDGEMENTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElixirProbeVariant {
    Zero,
    FirstStrike,
}

impl ElixirProbeVariant {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "zero" => Ok(Self::Zero),
            "first-strike" => Ok(Self::FirstStrike),
            _ => Err("Elixir probe variant must be zero or first-strike".to_owned()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::FirstStrike => "first_strike",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalProbeReport {
    pub schema: String,
    pub recipe_id: String,
    pub recipe_version: u32,
    pub provenance: String,
    pub variant: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub failure: Option<String>,
    pub expected_launch_plan: String,
    pub session: SessionManifest,
    pub steps: Vec<ProbeStepProof>,
    pub checks: Vec<ProbeCheck>,
    pub observations: ProbeObservations,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeStepProof {
    pub name: String,
    pub request_id: String,
    pub action_id: String,
    pub based_on_frame_id: u64,
    pub successor_frame_id: Option<u64>,
    pub status: String,
    pub settlement_wall_time_ms: Option<u128>,
    pub parameters: Value,
    pub receipt_path: String,
    pub receipt_sha256: String,
    pub successor_frame_path: Option<String>,
    pub successor_frame_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeFileIdentity {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatActionReport {
    pub schema: String,
    pub recipe_id: String,
    pub recipe_version: u32,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub failure: Option<String>,
    pub expected_launch_plan: String,
    pub session: SessionManifest,
    pub session_manifest: ProbeFileIdentity,
    pub initial_frame: ProbeFileIdentity,
    pub steps: Vec<ProbeStepProof>,
    pub checks: Vec<ProbeCheck>,
    pub compared_action_step_index: usize,
    pub action: NaturalCombatActionObservation,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalCombatActionObservation {
    pub action_id: String,
    pub card_uuid: String,
    pub card_id: String,
    pub hand_index: usize,
    pub target_monster_index: usize,
    pub target_monster_instance_id: u64,
    pub target_monster_id: String,
    pub from_frame_id: u64,
    pub to_frame_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeObservations {
    pub pre_elixir_frame_id: Option<u64>,
    pub hand_select_frame_id: Option<u64>,
    pub successor_frame_id: Option<u64>,
    pub selected_card_uuids: Vec<String>,
    pub pre_hand_card_uuids: Vec<String>,
    pub post_hand_card_uuids: Vec<String>,
    pub pre_exhaust_card_uuids: Vec<String>,
    pub post_exhaust_card_uuids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalProbeBundle {
    pub schema: String,
    pub recipe_id: String,
    pub recipe_version: u32,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub passed: bool,
    pub variants: Vec<ProbeBundleEntry>,
    pub shared_identity: ProbeSharedIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeBundleEntry {
    pub variant: String,
    pub session_id: String,
    pub report_path: String,
    pub report_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProbeSharedIdentity {
    pub source_commit: String,
    pub source_dirty: bool,
    pub loaded_mod_jar_sha256: String,
    pub sts_jar_sha256: String,
    pub mod_the_spire_jar_sha256: String,
    pub launch_profile: String,
    pub control_profile: String,
}

struct ProbeContext {
    report: NaturalProbeReport,
}

struct NaturalCombatContext {
    report: NaturalCombatActionReport,
}

trait ProbeRecorder {
    fn session(&self) -> &SessionManifest;
    fn steps(&self) -> &[ProbeStepProof];
    fn steps_mut(&mut self) -> &mut Vec<ProbeStepProof>;
    fn checks_mut(&mut self) -> &mut Vec<ProbeCheck>;

    fn record_receipt(&mut self, name: &str, receipt: &ActionReceipt) -> Result<(), String> {
        let paths = session_paths_from_manifest(self.session());
        let durable_receipt = receipt_path(&paths, &receipt.request_id);
        let successor_path = receipt.successor_frame_path.as_ref().map(PathBuf::from);
        self.steps_mut().push(ProbeStepProof {
            name: name.to_owned(),
            request_id: receipt.request_id.clone(),
            action_id: receipt.action_id.clone(),
            based_on_frame_id: receipt.based_on_frame_id,
            successor_frame_id: receipt.successor_frame_id,
            status: receipt.status.clone(),
            settlement_wall_time_ms: receipt.settlement_wall_time_ms,
            parameters: receipt.parameters.clone(),
            receipt_path: absolute_string(&durable_receipt)?,
            receipt_sha256: sha256_file(&durable_receipt)?,
            successor_frame_path: successor_path
                .as_ref()
                .map(|path| absolute_string(path))
                .transpose()?,
            successor_frame_sha256: successor_path
                .as_ref()
                .map(|path| sha256_file(path))
                .transpose()?,
        });
        if receipt.status != "completed"
            || receipt.successor_frame_id != Some(receipt.based_on_frame_id + 1)
        {
            return Err(format!(
                "probe step {name} did not complete with one immediate stable successor"
            ));
        }
        Ok(())
    }

    fn check(&mut self, name: &str, passed: bool, detail: impl Into<String>) {
        self.checks_mut().push(ProbeCheck {
            name: name.to_owned(),
            passed,
            detail: detail.into(),
        });
    }
}

impl ProbeRecorder for ProbeContext {
    fn session(&self) -> &SessionManifest {
        &self.report.session
    }

    fn steps(&self) -> &[ProbeStepProof] {
        &self.report.steps
    }

    fn steps_mut(&mut self) -> &mut Vec<ProbeStepProof> {
        &mut self.report.steps
    }

    fn checks_mut(&mut self) -> &mut Vec<ProbeCheck> {
        &mut self.report.checks
    }
}

impl ProbeRecorder for NaturalCombatContext {
    fn session(&self) -> &SessionManifest {
        &self.report.session
    }

    fn steps(&self) -> &[ProbeStepProof] {
        &self.report.steps
    }

    fn steps_mut(&mut self) -> &mut Vec<ProbeStepProof> {
        &mut self.report.steps
    }

    fn checks_mut(&mut self) -> &mut Vec<ProbeCheck> {
        &mut self.report.checks
    }
}

pub fn run_natural_base_strike_probe(
    output_dir: &Path,
    expected_launch_plan: &Path,
    report_path: &Path,
    timeout: Duration,
) -> Result<NaturalCombatActionReport, String> {
    let initial = status(output_dir)?;
    validate_initial_surface(&initial, expected_launch_plan)?;
    let created_at = unix_ms()?;
    let session_paths = session_paths_from_manifest(&initial.manifest);
    let mut context = NaturalCombatContext {
        report: NaturalCombatActionReport {
            schema: "sts_java_parity_natural_combat_action_report_v1".to_owned(),
            recipe_id: NATURAL_STRIKE_RECIPE_ID.to_owned(),
            recipe_version: NATURAL_STRIKE_RECIPE_VERSION,
            provenance: "natural_replay".to_owned(),
            created_at_unix_ms: created_at,
            completed_at_unix_ms: created_at,
            passed: false,
            failure: None,
            expected_launch_plan: absolute_string(expected_launch_plan)?,
            session: initial.manifest.clone(),
            session_manifest: probe_file_identity(&session_paths.manifest)?,
            initial_frame: probe_file_identity(Path::new(&initial.frame_path))?,
            steps: Vec::new(),
            checks: Vec::new(),
            compared_action_step_index: 0,
            action: NaturalCombatActionObservation::default(),
        },
    };

    let outcome = execute_natural_base_strike_recipe(output_dir, timeout, &mut context);
    context.report.completed_at_unix_ms = unix_ms()?;
    match outcome {
        Ok(()) => {
            context.report.passed = context.report.checks.iter().all(|check| check.passed);
            if !context.report.passed {
                context.report.failure =
                    Some("one or more natural combat invariants failed".to_owned());
            }
        }
        Err(error) => {
            context.report.failure = Some(error);
            context.report.passed = false;
        }
    }
    publish_json_create_new(report_path, &context.report)?;
    if context.report.passed {
        Ok(context.report)
    } else {
        Err(format!(
            "natural base Strike probe failed; durable report={}",
            absolute_string(report_path)?
        ))
    }
}

pub fn run_elixir_hand_select_probe(
    output_dir: &Path,
    variant: ElixirProbeVariant,
    expected_launch_plan: &Path,
    report_path: &Path,
    timeout: Duration,
) -> Result<NaturalProbeReport, String> {
    let initial = status(output_dir)?;
    validate_initial_surface(&initial, expected_launch_plan)?;
    let created_at = unix_ms()?;
    let mut context = ProbeContext {
        report: NaturalProbeReport {
            schema: "sts_java_parity_natural_probe_report_v1".to_owned(),
            recipe_id: RECIPE_ID.to_owned(),
            recipe_version: RECIPE_VERSION,
            provenance: "natural_replay".to_owned(),
            variant: variant.as_str().to_owned(),
            created_at_unix_ms: created_at,
            completed_at_unix_ms: created_at,
            passed: false,
            failure: None,
            expected_launch_plan: absolute_string(expected_launch_plan)?,
            session: initial.manifest.clone(),
            steps: Vec::new(),
            checks: Vec::new(),
            observations: ProbeObservations::default(),
        },
    };

    let outcome = execute_elixir_recipe(output_dir, variant, timeout, &mut context);
    context.report.completed_at_unix_ms = unix_ms()?;
    match outcome {
        Ok(()) => {
            context.report.passed = context.report.checks.iter().all(|check| check.passed);
            if !context.report.passed {
                context.report.failure = Some("one or more probe invariants failed".to_owned());
            }
        }
        Err(error) => {
            context.report.failure = Some(error);
            context.report.passed = false;
        }
    }
    publish_json_create_new(report_path, &context.report)?;
    if context.report.passed {
        Ok(context.report)
    } else {
        Err(format!(
            "Elixir probe failed; durable report={}",
            absolute_string(report_path)?
        ))
    }
}

pub fn bundle_elixir_hand_select_reports(
    report_paths: &[PathBuf],
    output_path: &Path,
) -> Result<NaturalProbeBundle, String> {
    if report_paths.len() != 2 {
        return Err("Elixir probe bundle requires exactly two variant reports".to_owned());
    }
    let mut reports = Vec::new();
    for path in report_paths {
        let bytes = fs::read(path)
            .map_err(|error| format!("failed to read probe report {}: {error}", path.display()))?;
        let report: NaturalProbeReport = serde_json::from_slice(&bytes)
            .map_err(|error| format!("failed to parse probe report {}: {error}", path.display()))?;
        reports.push((path, report));
    }
    let variants = reports
        .iter()
        .map(|(_, report)| report.variant.as_str())
        .collect::<BTreeSet<_>>();
    if variants != BTreeSet::from(["first_strike", "zero"])
        || reports.iter().any(|(_, report)| {
            !report.passed
                || report.recipe_id != RECIPE_ID
                || report.recipe_version != RECIPE_VERSION
                || report.provenance != "natural_replay"
        })
    {
        return Err("Elixir probe reports are not the two passing natural variants".to_owned());
    }
    if reports[0].1.session.session_id == reports[1].1.session.session_id {
        return Err("Elixir probe variants must use distinct fresh sessions".to_owned());
    }
    let identity = shared_identity(&reports[0].1)?;
    if shared_identity(&reports[1].1)? != identity {
        return Err("Elixir probe variants do not share one producer/game identity".to_owned());
    }
    let entries = reports
        .iter()
        .map(|(path, report)| {
            Ok(ProbeBundleEntry {
                variant: report.variant.clone(),
                session_id: report.session.session_id.clone(),
                report_path: absolute_string(path)?,
                report_sha256: sha256_file(path)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bundle = NaturalProbeBundle {
        schema: "sts_java_parity_natural_probe_bundle_v1".to_owned(),
        recipe_id: RECIPE_ID.to_owned(),
        recipe_version: RECIPE_VERSION,
        provenance: "natural_replay".to_owned(),
        created_at_unix_ms: unix_ms()?,
        passed: true,
        variants: entries,
        shared_identity: identity,
    };
    publish_json_create_new(output_path, &bundle)?;
    Ok(bundle)
}

fn execute_elixir_recipe(
    output_dir: &Path,
    variant: ElixirProbeVariant,
    timeout: Duration,
    context: &mut ProbeContext,
) -> Result<(), String> {
    let pre_elixir = execute_natural_run_to_first_combat(output_dir, timeout, context)?;
    context.report.observations.pre_elixir_frame_id = frame_id(&pre_elixir).ok();
    let potion_action = unique_action(&pre_elixir, "use Elixir", |action| {
        action.kind == "use_potion"
            && action.potion_id.as_deref() == Some("ElixirPotion")
            && action.potion_slot == Some(0)
    })?;
    let receipt = submit_action(&pre_elixir, potion_action, empty_object(), timeout)?;
    context.record_receipt("use_elixir", &receipt)?;

    let hand_select = ready_status(output_dir, timeout, context)?;
    require_decision(&hand_select, "hand_select")?;
    context.report.observations.hand_select_frame_id = frame_id(&hand_select).ok();
    let candidates = hand_select.frame["public_base"]["decision"]["hand_select"]["candidate_cards"]
        .as_array()
        .ok_or_else(|| "Elixir HAND_SELECT omitted candidate_cards".to_owned())?;
    let selected = match variant {
        ElixirProbeVariant::Zero => Vec::new(),
        ElixirProbeVariant::FirstStrike => vec![candidates
            .iter()
            .filter(|card| card["card_id"].as_str() == Some("Strike_R"))
            .min_by_key(|card| card["index"].as_u64().unwrap_or(u64::MAX))
            .and_then(|card| card["card_uuid"].as_str())
            .ok_or_else(|| "Elixir HAND_SELECT has no typed Strike_R candidate".to_owned())?
            .to_owned()],
    };
    context.report.observations.selected_card_uuids = selected.clone();
    let receipt = submit_hand_select(output_dir, &selected, timeout)?;
    context.record_receipt("submit_elixir_hand_select", &receipt)?;

    let successor = ready_status(output_dir, timeout, context)?;
    require_boundary(&successor, "combat_player_decision")?;
    context.report.observations.successor_frame_id = frame_id(&successor).ok();
    verify_elixir_transition(
        context,
        variant,
        &pre_elixir,
        &hand_select,
        &successor,
        &receipt,
    )?;
    verify_session_inventory(context, &successor)?;
    Ok(())
}

fn execute_natural_run_to_first_combat<C: ProbeRecorder>(
    output_dir: &Path,
    timeout: Duration,
    context: &mut C,
) -> Result<LiveStatus, String> {
    let receipt = start_run(output_dir, "IRONCLAD", 20, Some("0"), timeout)?;
    context.record_receipt("start_seed_0_a20_ironclad", &receipt)?;

    submit_matching_action(output_dir, timeout, context, "neow_talk", |action| {
        action.kind == "choose_event_option"
            && action.event_id.as_deref() == Some("Neow")
            && action.option_index == Some(0)
    })?;
    submit_matching_action(
        output_dir,
        timeout,
        context,
        "neow_choose_three_potions",
        |action| {
            action.kind == "choose_event_option"
                && action.event_id.as_deref() == Some("Neow")
                && action.option_index == Some(1)
        },
    )?;
    submit_matching_action(
        output_dir,
        timeout,
        context,
        "claim_first_elixir",
        |action| {
            action.kind == "claim_combat_reward"
                && action.reward_type.as_deref() == Some("POTION")
                && action.reward_id.as_deref() == Some("ElixirPotion")
                && action.reward_index == Some(0)
        },
    )?;
    submit_matching_action(
        output_dir,
        timeout,
        context,
        "claim_block_potion",
        |action| {
            action.kind == "claim_combat_reward"
                && action.reward_type.as_deref() == Some("POTION")
                && action.reward_id.as_deref() == Some("Block Potion")
                && action.reward_index == Some(1)
        },
    )?;
    submit_matching_action(
        output_dir,
        timeout,
        context,
        "close_neow_reward",
        |action| action.kind == "close_combat_reward",
    )?;
    submit_matching_action(
        output_dir,
        timeout,
        context,
        "choose_first_monster_node",
        |action| {
            action.kind == "choose_map_node" && action.map_x == Some(1) && action.map_y == Some(0)
        },
    )?;

    let combat = ready_status(output_dir, timeout, context)?;
    require_boundary(&combat, "combat_player_decision")?;
    Ok(combat)
}

fn execute_natural_base_strike_recipe(
    output_dir: &Path,
    timeout: Duration,
    context: &mut NaturalCombatContext,
) -> Result<(), String> {
    let before = execute_natural_run_to_first_combat(output_dir, timeout, context)?;
    let action = select_first_base_strike_action(&before)?.clone();
    let from_frame_id = frame_id(&before)?;
    context.report.compared_action_step_index = context.steps().len();
    let receipt = submit_action(&before, &action, empty_object(), timeout)?;
    context.record_receipt("play_first_base_strike", &receipt)?;
    let after = ready_status(output_dir, timeout, context)?;
    require_boundary(&after, "combat_player_decision")?;
    let to_frame_id = frame_id(&after)?;
    context.report.action = NaturalCombatActionObservation {
        action_id: action.action_id.clone(),
        card_uuid: action
            .card_uuid
            .clone()
            .ok_or_else(|| "selected base Strike omitted card UUID".to_owned())?,
        card_id: action
            .card_id
            .clone()
            .ok_or_else(|| "selected base Strike omitted card ID".to_owned())?,
        hand_index: action
            .hand_index
            .ok_or_else(|| "selected base Strike omitted hand index".to_owned())?,
        target_monster_index: action
            .target_monster_index
            .ok_or_else(|| "selected base Strike omitted target index".to_owned())?,
        target_monster_instance_id: action
            .target_monster_instance_id
            .ok_or_else(|| "selected base Strike omitted target instance ID".to_owned())?,
        target_monster_id: action
            .target_monster_id
            .clone()
            .ok_or_else(|| "selected base Strike omitted target ID".to_owned())?,
        from_frame_id,
        to_frame_id,
    };
    context.check(
        "typed_base_strike_action",
        action.kind == "play_card"
            && action.card_id.as_deref() == Some("Strike_R")
            && action.target_monster_index == Some(0)
            && receipt.action_id == action.action_id
            && receipt.parameters == empty_object(),
        format!(
            "action={} card_uuid={:?} hand_index={:?} target={:?}",
            action.action_id,
            action.card_uuid,
            action.hand_index,
            action.target_monster_instance_id
        ),
    );
    context.check(
        "adjacent_combat_successor",
        to_frame_id == from_frame_id + 1
            && receipt.based_on_frame_id == from_frame_id
            && receipt.successor_frame_id == Some(to_frame_id),
        format!("from={from_frame_id} to={to_frame_id}"),
    );
    verify_session_inventory(context, &after)?;
    Ok(())
}

fn select_first_base_strike_action(view: &LiveStatus) -> Result<&ActionHandle, String> {
    let minimum_hand_index = view
        .actions
        .iter()
        .filter(|action| {
            action.kind == "play_card"
                && action.card_id.as_deref() == Some("Strike_R")
                && action.target_monster_index == Some(0)
                && action
                    .card_uuid
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                && action.target_monster_instance_id.is_some()
                && action
                    .target_monster_id
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
        })
        .filter_map(|action| action.hand_index)
        .min()
        .ok_or_else(|| {
            "first natural combat has no complete typed base Strike action".to_owned()
        })?;
    unique_action(
        view,
        "minimum-hand-index base Strike at monster 0",
        |action| {
            action.kind == "play_card"
                && action.card_id.as_deref() == Some("Strike_R")
                && action.hand_index == Some(minimum_hand_index)
                && action.target_monster_index == Some(0)
                && action
                    .card_uuid
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                && action.target_monster_instance_id.is_some()
                && action
                    .target_monster_id
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
        },
    )
}

fn ready_status<C: ProbeRecorder>(
    output_dir: &Path,
    timeout: Duration,
    context: &mut C,
) -> Result<LiveStatus, String> {
    let mut acknowledgements = 0usize;
    loop {
        let view = status(output_dir)?;
        if view.frame["public_base"]["decision"]["kind"].as_str() != Some("ftue") {
            return Ok(view);
        }
        if acknowledgements == MAX_FTUE_ACKNOWLEDGEMENTS {
            return Err("natural probe exceeded its bounded FTUE acknowledgements".to_owned());
        }
        let action = unique_action(&view, "acknowledge FTUE", |action| {
            action.kind == "acknowledge_ftue"
        })?;
        let receipt = submit_action(&view, action, empty_object(), timeout)?;
        context.record_receipt("acknowledge_ftue", &receipt)?;
        acknowledgements += 1;
    }
}

fn submit_matching_action<C, F>(
    output_dir: &Path,
    timeout: Duration,
    context: &mut C,
    name: &str,
    predicate: F,
) -> Result<(), String>
where
    C: ProbeRecorder,
    F: Fn(&ActionHandle) -> bool,
{
    let view = ready_status(output_dir, timeout, context)?;
    let action = unique_action(&view, name, predicate)?;
    let receipt = submit_action(&view, action, empty_object(), timeout)?;
    context.record_receipt(name, &receipt)
}

fn unique_action<'a, F>(
    view: &'a LiveStatus,
    description: &str,
    predicate: F,
) -> Result<&'a ActionHandle, String>
where
    F: Fn(&ActionHandle) -> bool,
{
    let matches = view
        .actions
        .iter()
        .filter(|action| predicate(action))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [action] => Ok(*action),
        [] => Err(format!("no typed action matches {description}")),
        _ => Err(format!("more than one typed action matches {description}")),
    }
}

fn verify_elixir_transition(
    context: &mut ProbeContext,
    variant: ElixirProbeVariant,
    pre: &LiveStatus,
    hand_select: &LiveStatus,
    post: &LiveStatus,
    receipt: &ActionReceipt,
) -> Result<(), String> {
    let pre_hand = card_uuids(&pre.frame, "hand")?;
    let post_hand = card_uuids(&post.frame, "hand")?;
    let pre_exhaust = card_uuids(&pre.frame, "exhaust")?;
    let post_exhaust = card_uuids(&post.frame, "exhaust")?;
    context.report.observations.pre_hand_card_uuids = pre_hand.clone();
    context.report.observations.post_hand_card_uuids = post_hand.clone();
    context.report.observations.pre_exhaust_card_uuids = pre_exhaust.clone();
    context.report.observations.post_exhaust_card_uuids = post_exhaust.clone();

    let selected = context.report.observations.selected_card_uuids.clone();
    let expected_hand = pre_hand
        .iter()
        .filter(|uuid| !selected.contains(uuid))
        .cloned()
        .collect::<Vec<_>>();
    let mut expected_exhaust = pre_exhaust.clone();
    expected_exhaust.extend(selected.iter().cloned());
    expected_exhaust.sort();
    let mut actual_exhaust = post_exhaust.clone();
    actual_exhaust.sort();

    context.check(
        "selected_card_count",
        selected.len()
            == match variant {
                ElixirProbeVariant::Zero => 0,
                ElixirProbeVariant::FirstStrike => 1,
            },
        format!("selected={selected:?}"),
    );
    context.check(
        "hand_uuid_transition",
        post_hand == expected_hand,
        format!("before={pre_hand:?} after={post_hand:?}"),
    );
    context.check(
        "exhaust_uuid_transition",
        actual_exhaust == expected_exhaust,
        format!("before={pre_exhaust:?} after={post_exhaust:?}"),
    );
    context.check(
        "rng_unchanged",
        pre.frame["rng_internal"] == hand_select.frame["rng_internal"]
            && pre.frame["rng_internal"] == post.frame["rng_internal"],
        "pre-Elixir, HAND_SELECT, and successor RNG projections are byte-semantic equal",
    );
    context.check(
        "combat_scalar_state_unchanged",
        combat_stable_projection(&pre.frame)? == combat_stable_projection(&post.frame)?,
        "turn/player/draw/discard/monster state is unchanged",
    );
    context.check(
        "potion_transition",
        potion_transition_is_exact(&pre.frame, &post.frame)?,
        "slot 0 becomes empty and every other potion slot is unchanged",
    );
    context.check(
        "hand_select_has_no_partial_successor",
        receipt.successor_frame_id == Some(receipt.based_on_frame_id + 1)
            && frame_id(hand_select)? == receipt.based_on_frame_id
            && frame_id(post)? == receipt.successor_frame_id.unwrap_or_default(),
        format!(
            "based_on={} successor={:?}",
            receipt.based_on_frame_id, receipt.successor_frame_id
        ),
    );
    context.check(
        "hand_select_parameters_bind_uuid_order",
        receipt.parameters["selected_card_uuids"]
            == Value::Array(selected.iter().cloned().map(Value::String).collect()),
        format!("parameters={}", receipt.parameters),
    );
    Ok(())
}

fn verify_session_inventory<C: ProbeRecorder>(
    context: &mut C,
    final_view: &LiveStatus,
) -> Result<(), String> {
    let paths = session_paths_from_manifest(context.session());
    let mut frame_ids = fs::read_dir(&paths.frames_dir)
        .map_err(|error| format!("failed to enumerate probe frames: {error}"))?
        .map(|entry| {
            let name = entry
                .map_err(|error| format!("failed to read probe frame entry: {error}"))?
                .file_name()
                .to_string_lossy()
                .into_owned();
            name.strip_prefix("frame-")
                .and_then(|value| value.strip_suffix(".json"))
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or_else(|| format!("unexpected probe frame filename: {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    frame_ids.sort_unstable();
    let final_id = frame_id(final_view)?;
    context.check(
        "frame_inventory_contiguous",
        frame_ids == (1..=final_id).collect::<Vec<_>>(),
        format!("frame_ids={frame_ids:?}"),
    );

    let receipts = fs::read_dir(&paths.receipts_dir)
        .map_err(|error| format!("failed to enumerate probe receipts: {error}"))?
        .map(|entry| {
            let path = entry
                .map_err(|error| format!("failed to read probe receipt entry: {error}"))?
                .path();
            let bytes = fs::read(&path).map_err(|error| {
                format!("failed to read probe receipt {}: {error}", path.display())
            })?;
            serde_json::from_slice::<ActionReceipt>(&bytes).map_err(|error| {
                format!("failed to parse probe receipt {}: {error}", path.display())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    context.check(
        "receipt_inventory_exact",
        receipts.len() == context.steps().len()
            && receipts.iter().all(|receipt| receipt.status == "completed")
            && receipts
                .iter()
                .all(|receipt| receipt.successor_frame_id == Some(receipt.based_on_frame_id + 1)),
        format!(
            "durable_receipts={} recorded_steps={}",
            receipts.len(),
            context.steps().len()
        ),
    );
    Ok(())
}

fn probe_file_identity(path: &Path) -> Result<ProbeFileIdentity, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect probe file {}: {error}", path.display()))?;
    Ok(ProbeFileIdentity {
        path: absolute_string(path)?,
        bytes: metadata.len(),
        sha256: sha256_file(path)?,
    })
}

fn validate_initial_surface(view: &LiveStatus, expected_launch_plan: &Path) -> Result<(), String> {
    require_boundary(view, "main_menu_ready")?;
    if view.manifest.profile != "parity_run_control" {
        return Err("natural run probe requires parity_run_control".to_owned());
    }
    let launch = view
        .manifest
        .launch
        .as_ref()
        .ok_or_else(|| "natural run probe requires launch evidence".to_owned())?;
    if launch.profile != "accelerated"
        || launch.control_profile != "parity_run_control"
        || launch.storage_class != "isolated_working_directory_v1"
        || launch.isolation_id.is_none()
        || &launch.plan_path != expected_launch_plan
    {
        return Err(
            "natural run probe session does not match its expected accelerated isolated launch plan"
                .to_owned(),
        );
    }
    Ok(())
}

fn shared_identity(report: &NaturalProbeReport) -> Result<ProbeSharedIdentity, String> {
    let launch = report
        .session
        .launch
        .as_ref()
        .ok_or_else(|| "probe report omitted launch evidence".to_owned())?;
    Ok(ProbeSharedIdentity {
        source_commit: report.session.source_commit.clone(),
        source_dirty: report.session.source_dirty,
        loaded_mod_jar_sha256: report.session.loaded_mod_jar_sha256.clone(),
        sts_jar_sha256: report.session.sts_jar_sha256.clone(),
        mod_the_spire_jar_sha256: report.session.mod_the_spire_jar_sha256.clone(),
        launch_profile: launch.profile.clone(),
        control_profile: launch.control_profile.clone(),
    })
}

fn require_boundary(view: &LiveStatus, expected: &str) -> Result<(), String> {
    let actual = view.frame["boundary"]["kind"].as_str().unwrap_or("missing");
    if actual != expected {
        return Err(format!("expected boundary {expected}, found {actual}"));
    }
    Ok(())
}

fn require_decision(view: &LiveStatus, expected: &str) -> Result<(), String> {
    let actual = view.frame["public_base"]["decision"]["kind"]
        .as_str()
        .unwrap_or("missing");
    if actual != expected {
        return Err(format!("expected decision {expected}, found {actual}"));
    }
    Ok(())
}

fn frame_id(view: &LiveStatus) -> Result<u64, String> {
    view.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "probe frame omitted frame_id".to_owned())
}

fn card_uuids(frame: &Value, zone: &str) -> Result<Vec<String>, String> {
    let cards = match zone {
        "hand" => frame["public_base"]["combat"]["hand"].as_array(),
        "exhaust" => frame["public_base"]["combat"]["exhaust"]["cards"].as_array(),
        _ => None,
    }
    .ok_or_else(|| format!("combat frame omitted {zone} cards"))?;
    cards
        .iter()
        .map(|card| {
            card["instance_uuid"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{zone} card omitted instance_uuid"))
        })
        .collect()
}

fn combat_stable_projection(frame: &Value) -> Result<Value, String> {
    let combat = frame["public_base"]["combat"]
        .as_object()
        .ok_or_else(|| "probe frame omitted combat projection".to_owned())?;
    Ok(serde_json::json!({
        "turn": combat.get("turn"),
        "player": combat.get("player"),
        "draw_count": combat.get("draw_count"),
        "discard": combat.get("discard"),
        "monsters": combat.get("monsters"),
    }))
}

fn potion_transition_is_exact(pre: &Value, post: &Value) -> Result<bool, String> {
    let before = pre["public_base"]["combat"]["potions"]
        .as_array()
        .ok_or_else(|| "pre-Elixir frame omitted potions".to_owned())?;
    let after = post["public_base"]["combat"]["potions"]
        .as_array()
        .ok_or_else(|| "post-Elixir frame omitted potions".to_owned())?;
    if before.len() != after.len() || before.is_empty() {
        return Ok(false);
    }
    if before[0]["slot_index"].as_u64() != Some(0)
        || before[0]["potion_id"].as_str() != Some("ElixirPotion")
        || before[0]["empty"].as_bool() != Some(false)
        || after[0]["slot_index"].as_u64() != Some(0)
        || after[0]["empty"].as_bool() != Some(true)
    {
        return Ok(false);
    }
    Ok(before.iter().skip(1).eq(after.iter().skip(1)))
}

fn absolute_string(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?
            .join(path)
    };
    Ok(absolute.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targeted_strike(hand_index: usize, suffix: &str) -> ActionHandle {
        ActionHandle {
            handle: format!("f1:a{suffix}"),
            action_id: format!("play:card-{suffix}:target:monster-1"),
            kind: "play_card".to_owned(),
            card_uuid: Some(format!("card-{suffix}")),
            card_id: Some("Strike_R".to_owned()),
            card_upgrades: Some(0),
            card_ordinal: None,
            hand_index: Some(hand_index),
            potion_slot: None,
            potion_id: None,
            potion_potency: None,
            target_monster_id: Some("Cultist".to_owned()),
            target_monster_instance_id: Some(1),
            target_monster_index: Some(0),
            event_id: None,
            option_index: None,
            option_text: None,
            neow_reward_type: None,
            neow_drawback: None,
            map_x: None,
            map_y: None,
            map_room_type: None,
            reward_type: None,
            reward_index: None,
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

    fn selector_view(actions: Vec<ActionHandle>) -> LiveStatus {
        LiveStatus {
            manifest: SessionManifest {
                schema: "sts_java_parity_live_session_v3".to_owned(),
                session_id: "session".to_owned(),
                created_at_unix_ms: 1,
                process_id: 1,
                session_dir: "session".to_owned(),
                capture_id: "capture".to_owned(),
                producer_session_id: "producer".to_owned(),
                connection_id: "connection".to_owned(),
                profile: "parity_run_control".to_owned(),
                source_commit: "commit".to_owned(),
                source_dirty: false,
                loaded_mod_jar_sha256: "mod".to_owned(),
                sts_jar_sha256: "sts".to_owned(),
                mod_the_spire_jar_sha256: "mts".to_owned(),
                launch: None,
            },
            frame_path: "frame".to_owned(),
            frame: serde_json::json!({"frame_id": 1}),
            actions,
        }
    }

    #[test]
    fn variant_parser_is_closed() {
        assert_eq!(
            ElixirProbeVariant::parse("zero").unwrap(),
            ElixirProbeVariant::Zero
        );
        assert_eq!(
            ElixirProbeVariant::parse("first-strike").unwrap(),
            ElixirProbeVariant::FirstStrike
        );
        assert!(ElixirProbeVariant::parse("anything-else").is_err());
    }

    #[test]
    fn exact_potion_transition_requires_only_slot_zero_to_empty() {
        let before = serde_json::json!({"public_base":{"combat":{"potions":[
            {"slot_index":0,"potion_id":"ElixirPotion","empty":false},
            {"slot_index":1,"potion_id":"Block Potion","empty":false}
        ]}}});
        let after = serde_json::json!({"public_base":{"combat":{"potions":[
            {"slot_index":0,"potion_id":"Potion Slot","empty":true},
            {"slot_index":1,"potion_id":"Block Potion","empty":false}
        ]}}});
        assert!(potion_transition_is_exact(&before, &after).unwrap());

        let mut changed_other_slot = after;
        changed_other_slot["public_base"]["combat"]["potions"][1]["potion_id"] =
            Value::String("Dexterity Potion".to_owned());
        assert!(!potion_transition_is_exact(&before, &changed_other_slot).unwrap());
    }

    #[test]
    fn natural_strike_selector_uses_occurrence_identity_and_rejects_aliases() {
        let view = selector_view(vec![
            targeted_strike(4, "late"),
            targeted_strike(1, "first"),
        ]);
        let selected = select_first_base_strike_action(&view).unwrap();
        assert_eq!(selected.hand_index, Some(1));
        assert_eq!(selected.card_uuid.as_deref(), Some("card-first"));

        let duplicate = selector_view(vec![
            targeted_strike(1, "first"),
            targeted_strike(1, "alias"),
        ]);
        assert!(select_first_base_strike_action(&duplicate).is_err());
    }
}
