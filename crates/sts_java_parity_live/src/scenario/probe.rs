use super::*;
use std::collections::BTreeSet;

const COMBAT_END_TURN_TRANSITION: &str = "combat.end_turn.v1";
const COMBAT_PLAY_CARD_TRANSITION: &str = "combat.play_card.v1";
const COMBAT_USE_POTION_TRANSITION: &str = "combat.use_potion.v1";
const PROBE_DRAFT_SCHEMA: &str = "sts_java_play_scenario_probe_draft_v1";
const PROBE_PLAN_SCHEMA: &str = "sts_java_play_scenario_probe_plan_v2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "transition_id", deny_unknown_fields)]
pub enum ScenarioProbeTransition {
    #[serde(rename = "combat.end_turn.v1")]
    CombatEndTurn,
    #[serde(rename = "combat.play_card.v1")]
    CombatPlayCard {
        card: ScenarioProbeCardSelector,
        target: Option<ScenarioProbeMonsterSelector>,
    },
    #[serde(rename = "combat.use_potion.v1")]
    CombatUsePotion {
        potion: ScenarioProbePotionSelector,
        target: Option<ScenarioProbeMonsterSelector>,
    },
}

impl ScenarioProbeTransition {
    pub fn id(&self) -> &'static str {
        match self {
            Self::CombatEndTurn => COMBAT_END_TURN_TRANSITION,
            Self::CombatPlayCard { .. } => COMBAT_PLAY_CARD_TRANSITION,
            Self::CombatUsePotion { .. } => COMBAT_USE_POTION_TRANSITION,
        }
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        match self {
            Self::CombatEndTurn => Ok(()),
            Self::CombatPlayCard { card, target } => {
                card.validate()?;
                if let Some(target) = target {
                    target.validate()?;
                }
                Ok(())
            }
            Self::CombatUsePotion { potion, target } => {
                potion.validate()?;
                if let Some(target) = target {
                    target.validate()?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioProbeCardSelector {
    pub card_id: String,
    pub upgrades: u32,
    pub occurrence: usize,
}

impl ScenarioProbeCardSelector {
    fn validate(&self) -> Result<(), String> {
        if self.card_id.is_empty() || self.upgrades > 20 || self.occurrence >= 100 {
            return Err("card selector is outside the closed exact-occurrence schema".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioProbeMonsterSelector {
    pub monster_id: String,
    pub occurrence: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioProbePotionSelector {
    pub slot_index: usize,
    pub potion_id: String,
}

impl ScenarioProbePotionSelector {
    fn validate(&self) -> Result<(), String> {
        if self.slot_index >= MAX_SCENARIO_POTION_SLOTS
            || self.potion_id.is_empty()
            || self.potion_id == "Potion Slot"
        {
            return Err("potion selector is outside the closed exact-slot schema".to_owned());
        }
        Ok(())
    }
}

impl ScenarioProbeMonsterSelector {
    fn validate(&self) -> Result<(), String> {
        if self.monster_id.is_empty() || self.occurrence >= 20 {
            return Err(
                "monster selector is outside the closed exact-occurrence schema".to_owned(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioProbeDraft {
    pub schema: String,
    pub provenance: String,
    pub probe_case_id: String,
    pub scenario_case_id: String,
    pub transition: ScenarioProbeTransition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioProbePlan {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub probe_case_id: String,
    pub probe_draft_path: String,
    pub probe_draft_sha256: String,
    pub scenario_case_id: String,
    pub scenario_plan_path: String,
    pub scenario_plan_sha256: String,
    pub transition: ScenarioProbeTransition,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioProbeCaseCatalog {
    pub schema: &'static str,
    pub catalog_directory: String,
    pub cases: Vec<ScenarioProbeCaseCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioProbeCaseCatalogEntry {
    pub probe_case_id: String,
    pub draft_file: String,
    pub draft_sha256: String,
    pub provenance: String,
    pub scenario_case_id: String,
    pub transition: ScenarioProbeTransition,
}

pub(super) fn scenario_transition_descriptors() -> Vec<ScenarioTransitionDescriptor> {
    vec![
        ScenarioTransitionDescriptor {
            transition_id: COMBAT_END_TURN_TRANSITION,
            phase: "after_constructed_root",
            action_kind: "end_turn",
            parameters: "empty_object",
            cardinality: "exactly_one",
        },
        ScenarioTransitionDescriptor {
            transition_id: COMBAT_PLAY_CARD_TRANSITION,
            phase: "after_constructed_root",
            action_kind: "play_card",
            parameters: "exact_card_occurrence_optional_exact_target_occurrence_empty_object",
            cardinality: "exactly_one",
        },
        ScenarioTransitionDescriptor {
            transition_id: COMBAT_USE_POTION_TRANSITION,
            phase: "after_constructed_root",
            action_kind: "use_potion",
            parameters: "exact_potion_slot_and_id_optional_exact_target_occurrence_empty_object",
            cardinality: "exactly_one",
        },
    ]
}

pub fn scenario_probe_case_catalog(
    catalog_directory: &Path,
) -> Result<ScenarioProbeCaseCatalog, String> {
    let entries = fs::read_dir(catalog_directory).map_err(|error| {
        format!(
            "failed to read scenario probe case catalog {}: {error}",
            catalog_directory.display()
        )
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to enumerate scenario probe case catalog {}: {error}",
                catalog_directory.display()
            )
        })?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| {
                format!(
                    "failed to inspect scenario probe case {}: {error}",
                    path.display()
                )
            })?
            .is_file()
            && path.extension().and_then(|value| value.to_str()) == Some("json")
        {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err("scenario probe case catalog contains no JSON drafts".to_owned());
    }
    let mut ids = BTreeSet::new();
    let mut cases = Vec::with_capacity(paths.len());
    for path in paths {
        let draft = read_and_validate_probe_draft(&path)?;
        let expected_file = format!("{}.json", draft.probe_case_id);
        let draft_file = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                format!(
                    "scenario probe case filename is not valid Unicode: {}",
                    path.display()
                )
            })?;
        if draft_file != expected_file {
            return Err(format!(
                "scenario probe case filename must exactly match embedded probe_case_id: expected {expected_file}, found {draft_file}"
            ));
        }
        if !ids.insert(draft.probe_case_id.clone()) {
            return Err(format!(
                "scenario probe case catalog contains duplicate probe_case_id: {}",
                draft.probe_case_id
            ));
        }
        cases.push(ScenarioProbeCaseCatalogEntry {
            probe_case_id: draft.probe_case_id,
            draft_file: draft_file.to_owned(),
            draft_sha256: sha256_file(&path)?,
            provenance: draft.provenance,
            scenario_case_id: draft.scenario_case_id,
            transition: draft.transition,
        });
    }
    cases.sort_by(|left, right| left.probe_case_id.cmp(&right.probe_case_id));
    Ok(ScenarioProbeCaseCatalog {
        schema: "sts_java_parity_scenario_probe_case_catalog_v1",
        catalog_directory: absolute_string(catalog_directory)?,
        cases,
    })
}

pub fn compile_scenario_probe_case(
    probe_catalog_directory: &Path,
    probe_case_id: &str,
    scenario_catalog_directory: &Path,
    scenario_plan_path: &Path,
    probe_plan_path: &Path,
) -> Result<ScenarioProbePlan, String> {
    if !valid_case_id(probe_case_id) {
        return Err("scenario probe case ID is outside the exact lowercase slug schema".to_owned());
    }
    let catalog = scenario_probe_case_catalog(probe_catalog_directory)?;
    let entry = catalog
        .cases
        .iter()
        .find(|entry| entry.probe_case_id == probe_case_id)
        .ok_or_else(|| {
            format!("scenario probe catalog has no exact probe_case_id '{probe_case_id}'")
        })?;
    let probe_draft_path = probe_catalog_directory.join(&entry.draft_file);
    let draft = read_and_validate_probe_draft(&probe_draft_path)?;
    if draft.probe_case_id != probe_case_id {
        return Err("scenario probe identity changed during exact catalog resolution".to_owned());
    }
    let scenario_catalog = scenario_case_catalog(scenario_catalog_directory)?;
    let scenario_entry = scenario_catalog
        .cases
        .iter()
        .find(|candidate| candidate.case_id == draft.scenario_case_id)
        .ok_or_else(|| {
            format!(
                "scenario probe references missing exact scenario_case_id '{}'",
                draft.scenario_case_id
            )
        })?;
    let scenario_draft =
        read_and_validate_draft(&scenario_catalog_directory.join(&scenario_entry.draft_file))?;
    validate_transition_root_dependencies(&draft.transition, &scenario_draft)?;
    let scenario_plan = compile_scenario_case(
        scenario_catalog_directory,
        &draft.scenario_case_id,
        scenario_plan_path,
    )?;
    if scenario_plan.recipe.case_id != draft.scenario_case_id {
        return Err("compiled scenario plan disagrees with its probe case".to_owned());
    }
    let plan = ScenarioProbePlan {
        schema: PROBE_PLAN_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: unix_ms()?,
        probe_case_id: draft.probe_case_id,
        probe_draft_path: absolute_string(&probe_draft_path)?,
        probe_draft_sha256: sha256_file(&probe_draft_path)?,
        scenario_case_id: draft.scenario_case_id,
        scenario_plan_path: absolute_string(scenario_plan_path)?,
        scenario_plan_sha256: sha256_file(scenario_plan_path)?,
        transition: draft.transition,
    };
    publish_json_create_new(probe_plan_path, &plan)?;
    Ok(plan)
}

pub(super) fn validate_transition_root_dependencies(
    transition: &ScenarioProbeTransition,
    scenario_draft: &ScenarioDraft,
) -> Result<(), String> {
    match transition {
        ScenarioProbeTransition::CombatEndTurn => Ok(()),
        ScenarioProbeTransition::CombatPlayCard { card, .. } => {
            let deck = scenario_draft
                .recipe
                .capabilities
                .iter()
                .find_map(|capability| match capability {
                    ScenarioCapability::MasterDeck(deck) => Some(deck),
                    _ => None,
                })
                .ok_or_else(|| "scenario probe root omitted its required exact deck".to_owned())?;
            let available = deck
                .cards
                .iter()
                .filter(|candidate| {
                    candidate.card_id == card.card_id && candidate.upgrades == card.upgrades
                })
                .count();
            if available <= card.occurrence {
                return Err(format!(
                    "scenario root deck has {available} copies of exact card {}+{} but probe selects occurrence {}",
                    card.card_id, card.upgrades, card.occurrence
                ));
            }
            Ok(())
        }
        ScenarioProbeTransition::CombatUsePotion { potion, .. } => {
            let slots = scenario_draft
                .recipe
                .capabilities
                .iter()
                .find_map(|capability| match capability {
                    ScenarioCapability::FreshPotions(potions) => Some(&potions.slots),
                    _ => None,
                })
                .ok_or_else(|| "potion probe root omitted fresh exact potion slots".to_owned())?;
            let selected = slots.get(potion.slot_index).ok_or_else(|| {
                "potion probe selects a slot outside the exact constructed inventory".to_owned()
            })?;
            if selected.slot_index != potion.slot_index
                || selected.potion_id != potion.potion_id
                || selected.potion_id == "Potion Slot"
            {
                return Err(
                    "potion probe selector disagrees with the exact constructed inventory"
                        .to_owned(),
                );
            }
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedScenarioProbeReport {
    pub schema: String,
    pub provenance: String,
    pub evidence_scope: ScenarioProbeEvidenceScope,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub failure: Option<String>,
    pub probe_case_id: String,
    pub probe_plan_path: String,
    pub probe_plan_sha256: String,
    pub scenario_report_path: String,
    pub scenario_report_sha256: String,
    pub transition_id: String,
    pub transition: ScenarioProbeTransition,
    pub session: SessionManifest,
    pub root_frame_id: u64,
    pub action: Option<ScenarioStepProof>,
    pub checks: Vec<ScenarioCheck>,
    pub successor: Option<ScenarioRootProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioProbeEvidenceScope {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub constructed_root_verified: bool,
    pub original_game_transition_captured: bool,
    pub simulator_comparison: bool,
}

impl ScenarioProbeEvidenceScope {
    fn after_verified_root(transition_captured: bool) -> Self {
        Self {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            constructed_root_verified: true,
            original_game_transition_captured: transition_captured,
            simulator_comparison: false,
        }
    }
}

pub fn run_constructed_combat_probe(
    output_dir: &Path,
    probe_plan_path: &Path,
    expected_launch_plan: &Path,
    scenario_report_path: &Path,
    probe_report_path: &Path,
    timeout: Duration,
) -> Result<ConstructedScenarioProbeReport, String> {
    let probe_plan = read_and_validate_probe_plan(probe_plan_path)?;
    let scenario_plan_path = PathBuf::from(&probe_plan.scenario_plan_path);
    let transition = probe_plan.transition.clone();
    let root_report = run_constructed_combat_scenario(
        output_dir,
        &scenario_plan_path,
        expected_launch_plan,
        scenario_report_path,
        timeout,
    )?;
    let root = root_report
        .root
        .as_ref()
        .ok_or_else(|| "passing constructed scenario report omitted its root".to_owned())?;
    if root.scenario_context["case_id"].as_str() != Some(probe_plan.scenario_case_id.as_str()) {
        return Err("constructed probe root does not match its bound scenario case".to_owned());
    }
    let created_at = unix_ms()?;
    let mut report = ConstructedScenarioProbeReport {
        schema: "sts_java_parity_constructed_probe_report_v2".to_owned(),
        provenance: "constructed_validated".to_owned(),
        evidence_scope: ScenarioProbeEvidenceScope::after_verified_root(false),
        created_at_unix_ms: created_at,
        completed_at_unix_ms: created_at,
        passed: false,
        failure: None,
        probe_case_id: probe_plan.probe_case_id,
        probe_plan_path: absolute_string(probe_plan_path)?,
        probe_plan_sha256: sha256_file(probe_plan_path)?,
        scenario_report_path: absolute_string(scenario_report_path)?,
        scenario_report_sha256: sha256_file(scenario_report_path)?,
        transition_id: transition.id().to_owned(),
        transition: transition.clone(),
        session: root_report.session.clone(),
        root_frame_id: root.frame_id,
        action: None,
        checks: Vec::new(),
        successor: None,
    };

    let outcome = execute_probe(output_dir, &transition, &root_report, timeout, &mut report);
    report.completed_at_unix_ms = unix_ms()?;
    match outcome {
        Ok(()) => {
            report.passed = report.checks.iter().all(|check| check.passed);
            report.evidence_scope = ScenarioProbeEvidenceScope::after_verified_root(report.passed);
            if !report.passed {
                report.failure = Some("one or more constructed probe invariants failed".to_owned());
            }
        }
        Err(error) => report.failure = Some(error),
    }
    publish_json_create_new(probe_report_path, &report)?;
    if report.passed {
        Ok(report)
    } else {
        Err(format!(
            "constructed scenario probe failed; durable report={}",
            probe_report_path.display()
        ))
    }
}

fn read_and_validate_probe_draft(path: &Path) -> Result<ScenarioProbeDraft, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read scenario probe draft {}: {error}",
            path.display()
        )
    })?;
    let draft: ScenarioProbeDraft = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse scenario probe draft {}: {error}",
            path.display()
        )
    })?;
    if draft.schema != PROBE_DRAFT_SCHEMA
        || draft.provenance != "constructed_validated"
        || !valid_case_id(&draft.probe_case_id)
        || !valid_case_id(&draft.scenario_case_id)
    {
        return Err("scenario probe draft identity or provenance is invalid".to_owned());
    }
    draft.transition.validate()?;
    Ok(draft)
}

pub(super) fn read_and_validate_probe_plan(path: &Path) -> Result<ScenarioProbePlan, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read scenario probe plan {}: {error}",
            path.display()
        )
    })?;
    let plan: ScenarioProbePlan = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse scenario probe plan {}: {error}",
            path.display()
        )
    })?;
    if plan.schema != PROBE_PLAN_SCHEMA
        || plan.provenance != "constructed_validated"
        || plan.created_at_unix_ms == 0
        || !valid_case_id(&plan.probe_case_id)
        || !valid_case_id(&plan.scenario_case_id)
    {
        return Err("scenario probe plan identity is invalid".to_owned());
    }
    plan.transition.validate()?;
    let probe_draft_path = PathBuf::from(&plan.probe_draft_path);
    if sha256_file(&probe_draft_path)? != plan.probe_draft_sha256 {
        return Err("scenario probe draft changed after plan compilation".to_owned());
    }
    let draft = read_and_validate_probe_draft(&probe_draft_path)?;
    if draft.probe_case_id != plan.probe_case_id
        || draft.scenario_case_id != plan.scenario_case_id
        || draft.transition != plan.transition
    {
        return Err("scenario probe plan disagrees with its bound draft".to_owned());
    }
    let scenario_plan_path = PathBuf::from(&plan.scenario_plan_path);
    if sha256_file(&scenario_plan_path)? != plan.scenario_plan_sha256 {
        return Err("scenario plan changed after probe plan compilation".to_owned());
    }
    let scenario_plan = read_and_validate_plan(&scenario_plan_path)?;
    if scenario_plan.recipe.case_id != plan.scenario_case_id {
        return Err("scenario probe plan disagrees with its bound scenario plan".to_owned());
    }
    Ok(plan)
}

fn execute_probe(
    output_dir: &Path,
    transition: &ScenarioProbeTransition,
    root_report: &ConstructedScenarioReport,
    timeout: Duration,
    report: &mut ConstructedScenarioProbeReport,
) -> Result<(), String> {
    let root = root_report.root.as_ref().unwrap();
    let before = status(output_dir)?;
    if frame_id(&before)? != root.frame_id
        || boundary_kind(&before) != Some("combat_player_decision")
        || before.frame["scenario"] != root.scenario_context
    {
        return Err("constructed probe no longer owns the verified combat root".to_owned());
    }
    let action = resolve_scenario_transition_action(&before, transition)?;
    let receipt = submit_action(&before, action, empty_object(), timeout)?;
    report.action = Some(scenario_step_proof(
        &root_report.session,
        "probe_transition",
        &receipt,
    )?);
    require_completed_successor(&receipt, transition.id())?;

    let after = status(output_dir)?;
    let after_frame_id = frame_id(&after)?;
    report.checks.push(ScenarioCheck {
        name: "probe_has_adjacent_stable_successor".to_owned(),
        passed: receipt.successor_frame_id == Some(root.frame_id + 1)
            && receipt.successor_frame_id == Some(after_frame_id)
            && boundary_kind(&after) == Some("combat_player_decision"),
        detail: format!(
            "root={} receipt_successor={:?} latest={after_frame_id}",
            root.frame_id, receipt.successor_frame_id
        ),
    });
    report.checks.push(ScenarioCheck {
        name: "probe_preserves_constructed_provenance".to_owned(),
        passed: after.frame["scenario"] == root.scenario_context,
        detail: format!("scenario_id={}", root.scenario_context["scenario_id"]),
    });
    report.successor = Some(ScenarioRootProof {
        frame_id: after_frame_id,
        frame_path: absolute_string(Path::new(&after.frame_path))?,
        frame_sha256: sha256_file(Path::new(&after.frame_path))?,
        boundary: boundary_kind(&after).unwrap_or_default().to_owned(),
        scenario_context: after.frame["scenario"].clone(),
        observed_player: after.frame["public_base"]["combat"]["player"].clone(),
        observed_monsters: after.frame["public_base"]["combat"]["monsters"].clone(),
        observed_relics: after.frame["parity_internal"]["relics"].clone(),
        observed_potions: after.frame["public_base"]["combat"]["potions"].clone(),
    });
    Ok(())
}

pub(super) fn resolve_scenario_transition_action<'a>(
    view: &'a LiveStatus,
    transition: &ScenarioProbeTransition,
) -> Result<&'a ActionHandle, String> {
    match transition {
        ScenarioProbeTransition::CombatEndTurn => {
            let action = unique_action(view, "end_turn")?;
            if action.action_id != "end_turn" || action.parameters_required {
                return Err(
                    "typed combat end-turn transition did not resolve exact end_turn".to_owned(),
                );
            }
            Ok(action)
        }
        ScenarioProbeTransition::CombatPlayCard { card, target } => {
            resolve_play_card_action(&view.frame, &view.actions, card, target.as_ref())
        }
        ScenarioProbeTransition::CombatUsePotion { potion, target } => {
            resolve_use_potion_action(&view.frame, &view.actions, potion, target.as_ref())
        }
    }
}

fn resolve_use_potion_action<'a>(
    frame: &Value,
    actions: &'a [ActionHandle],
    potion_selector: &ScenarioProbePotionSelector,
    target_selector: Option<&ScenarioProbeMonsterSelector>,
) -> Result<&'a ActionHandle, String> {
    let potions = frame["public_base"]["combat"]["potions"]
        .as_array()
        .ok_or_else(|| "constructed combat frame has no typed potion slots".to_owned())?;
    let selected_potion = potions
        .get(potion_selector.slot_index)
        .ok_or_else(|| "constructed combat frame has no selected typed potion slot".to_owned())?;
    if selected_potion["slot_index"].as_u64() != u64::try_from(potion_selector.slot_index).ok()
        || selected_potion["potion_slot"].as_u64() != u64::try_from(potion_selector.slot_index).ok()
        || selected_potion["potion_id"].as_str() != Some(potion_selector.potion_id.as_str())
        || selected_potion["empty"].as_bool() != Some(false)
    {
        return Err("selected potion slot disagrees with its typed root observation".to_owned());
    }
    let potency = selected_potion["potency"]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| "selected potion has no typed potency observation".to_owned())?;
    let selected_target = resolve_monster_selector(frame, target_selector)?;
    let mut matches = actions.iter().filter(|action| {
        action.kind == "use_potion"
            && action.potion_slot == Some(potion_selector.slot_index)
            && action.potion_id.as_deref() == Some(potion_selector.potion_id.as_str())
            && action.potion_potency == Some(potency)
            && !action.parameters_required
            && action_target_matches(action, selected_target.as_ref())
    });
    let action = matches.next().ok_or_else(|| {
        "exact typed potion/target selector matched no authorized action".to_owned()
    })?;
    if matches.next().is_some() {
        return Err(
            "exact typed potion/target selector matched multiple authorized actions".to_owned(),
        );
    }
    Ok(action)
}

fn resolve_play_card_action<'a>(
    frame: &Value,
    actions: &'a [ActionHandle],
    card_selector: &ScenarioProbeCardSelector,
    target_selector: Option<&ScenarioProbeMonsterSelector>,
) -> Result<&'a ActionHandle, String> {
    let hand = frame["parity_internal"]["zones"]["hand"]["cards"]
        .as_array()
        .ok_or_else(|| "constructed combat frame has no typed hand zone".to_owned())?;
    let selected_card = hand
        .iter()
        .enumerate()
        .filter(|(_, card)| {
            card["card_id"].as_str() == Some(card_selector.card_id.as_str())
                && card["upgrades"].as_u64() == Some(u64::from(card_selector.upgrades))
        })
        .nth(card_selector.occurrence)
        .ok_or_else(|| {
            format!(
                "hand has no occurrence {} of exact card {}+{}",
                card_selector.occurrence, card_selector.card_id, card_selector.upgrades
            )
        })?;
    let hand_index = selected_card.0;
    let card_uuid = selected_card.1["instance_uuid"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "selected hand card has no instance UUID".to_owned())?;

    let selected_target = resolve_monster_selector(frame, target_selector)?;

    let mut matches = actions.iter().filter(|action| {
        action.kind == "play_card"
            && action.card_uuid.as_deref() == Some(card_uuid)
            && action.card_id.as_deref() == Some(card_selector.card_id.as_str())
            && action.hand_index == Some(hand_index)
            && !action.parameters_required
            && action_target_matches(action, selected_target.as_ref())
    });
    let action = matches.next().ok_or_else(|| {
        "exact typed card/target selector matched no authorized action".to_owned()
    })?;
    if matches.next().is_some() {
        return Err(
            "exact typed card/target selector matched multiple authorized actions".to_owned(),
        );
    }
    Ok(action)
}

fn resolve_monster_selector(
    frame: &Value,
    selector: Option<&ScenarioProbeMonsterSelector>,
) -> Result<Option<(u64, usize, String)>, String> {
    let Some(selector) = selector else {
        return Ok(None);
    };
    let monsters = frame["public_base"]["combat"]["monsters"]
        .as_array()
        .ok_or_else(|| "constructed combat frame has no typed monster list".to_owned())?;
    let target = monsters
        .iter()
        .filter(|monster| {
            monster["id"].as_str() == Some(selector.monster_id.as_str())
                && monster["dead_or_escaped"].as_bool() == Some(false)
        })
        .nth(selector.occurrence)
        .ok_or_else(|| {
            format!(
                "combat has no live occurrence {} of exact monster {}",
                selector.occurrence, selector.monster_id
            )
        })?;
    Ok(Some((
        target["instance_id"]
            .as_u64()
            .ok_or_else(|| "selected monster has no instance ID".to_owned())?,
        target["index"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| "selected monster has no valid index".to_owned())?,
        selector.monster_id.clone(),
    )))
}

fn action_target_matches(
    action: &ActionHandle,
    selected_target: Option<&(u64, usize, String)>,
) -> bool {
    match selected_target {
        Some((instance_id, index, monster_id)) => {
            action.target_monster_instance_id == Some(*instance_id)
                && action.target_monster_index == Some(*index)
                && action.target_monster_id.as_deref() == Some(monster_id.as_str())
        }
        None => {
            action.target_monster_instance_id.is_none()
                && action.target_monster_index.is_none()
                && action.target_monster_id.is_none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end_turn_draft() -> ScenarioProbeDraft {
        ScenarioProbeDraft {
            schema: PROBE_DRAFT_SCHEMA.to_owned(),
            provenance: "constructed_validated".to_owned(),
            probe_case_id: "test-cultist-end-turn".to_owned(),
            scenario_case_id: "test-cultist-root".to_owned(),
            transition: ScenarioProbeTransition::CombatEndTurn,
        }
    }

    fn action(
        index: usize,
        card_uuid: &str,
        hand_index: usize,
        target_instance_id: u64,
    ) -> ActionHandle {
        ActionHandle {
            handle: format!("f4:a{index}"),
            action_id: format!("play_card:{card_uuid}:monster:{target_instance_id}"),
            kind: "play_card".to_owned(),
            card_uuid: Some(card_uuid.to_owned()),
            card_id: Some("Strike_R".to_owned()),
            card_upgrades: Some(0),
            card_ordinal: None,
            hand_index: Some(hand_index),
            potion_slot: None,
            potion_id: None,
            potion_potency: None,
            target_monster_id: Some("Cultist".to_owned()),
            target_monster_instance_id: Some(target_instance_id),
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

    #[test]
    fn transition_registry_and_serde_share_closed_identities() {
        let end_turn: ScenarioProbeTransition = serde_json::from_value(serde_json::json!({
            "transition_id": "combat.end_turn.v1"
        }))
        .unwrap();
        assert_eq!(end_turn.id(), COMBAT_END_TURN_TRANSITION);
        let play_card: ScenarioProbeTransition = serde_json::from_value(serde_json::json!({
            "transition_id": "combat.play_card.v1",
            "card": {"card_id": "Strike_R", "upgrades": 0, "occurrence": 1},
            "target": {"monster_id": "Cultist", "occurrence": 0}
        }))
        .unwrap();
        play_card.validate().unwrap();
        let use_potion: ScenarioProbeTransition = serde_json::from_value(serde_json::json!({
            "transition_id": "combat.use_potion.v1",
            "potion": {"slot_index": 0, "potion_id": "Block Potion"},
            "target": null
        }))
        .unwrap();
        use_potion.validate().unwrap();
        assert_eq!(scenario_transition_descriptors().len(), 3);
        assert!(
            serde_json::from_value::<ScenarioProbeTransition>(serde_json::json!({
                "transition_id": "play strike"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<ScenarioProbeTransition>(serde_json::json!({
                "transition_id": "combat.play_card.v1",
                "card": {"card_id": "Whirlwind", "upgrades": 0, "occurrence": 0},
                "target": null,
                "energy": 1
            }))
            .is_err(),
            "closed card-play selectors must not encode a caller-chosen X value"
        );
    }

    #[test]
    fn exact_occurrence_selector_disambiguates_duplicate_cards_without_uuid_in_fixture() {
        let frame = serde_json::json!({
            "parity_internal": {"zones": {"hand": {"cards": [
                {"instance_uuid": "first", "card_id": "Strike_R", "upgrades": 0},
                {"instance_uuid": "bash", "card_id": "Bash", "upgrades": 0},
                {"instance_uuid": "second", "card_id": "Strike_R", "upgrades": 0}
            ]}}},
            "public_base": {"combat": {"monsters": [
                {"id": "Cultist", "instance_id": 7, "index": 0, "dead_or_escaped": false}
            ]}}
        });
        let actions = vec![action(0, "first", 0, 7), action(1, "second", 2, 7)];
        let selected = resolve_play_card_action(
            &frame,
            &actions,
            &ScenarioProbeCardSelector {
                card_id: "Strike_R".to_owned(),
                upgrades: 0,
                occurrence: 1,
            },
            Some(&ScenarioProbeMonsterSelector {
                monster_id: "Cultist".to_owned(),
                occurrence: 0,
            }),
        )
        .unwrap();
        assert_eq!(selected.card_uuid.as_deref(), Some("second"));
        assert_eq!(selected.hand_index, Some(2));
    }

    #[test]
    fn exact_potion_selector_uses_runtime_potency_without_fixture_action_id() {
        let frame = serde_json::json!({
            "public_base": {"combat": {
                "potions": [
                    {
                        "slot_index": 0,
                        "potion_slot": 0,
                        "potion_id": "Block Potion",
                        "empty": false,
                        "potency": 12
                    },
                    {
                        "slot_index": 1,
                        "potion_slot": -1,
                        "potion_id": "Potion Slot",
                        "empty": true,
                        "potency": 0
                    }
                ],
                "monsters": []
            }}
        });
        let mut candidate = action(0, "unused", 0, 7);
        candidate.action_id = "runtime-owned-id".to_owned();
        candidate.kind = "use_potion".to_owned();
        candidate.card_uuid = None;
        candidate.card_id = None;
        candidate.hand_index = None;
        candidate.potion_slot = Some(0);
        candidate.potion_id = Some("Block Potion".to_owned());
        candidate.potion_potency = Some(12);
        candidate.target_monster_id = None;
        candidate.target_monster_instance_id = None;
        candidate.target_monster_index = None;

        let candidates = vec![candidate];
        let selected = resolve_use_potion_action(
            &frame,
            &candidates,
            &ScenarioProbePotionSelector {
                slot_index: 0,
                potion_id: "Block Potion".to_owned(),
            },
            None,
        )
        .unwrap();
        assert_eq!(selected.potion_potency, Some(12));
        assert_eq!(selected.action_id, "runtime-owned-id");
    }

    #[test]
    fn captured_transition_scope_does_not_claim_simulator_comparison() {
        let scope = ScenarioProbeEvidenceScope::after_verified_root(true);
        assert!(scope.constructed_root_verified);
        assert!(scope.original_game_transition_captured);
        assert!(!scope.simulator_comparison);
        assert!(!scope.natural_reachability);
        assert!(!scope.training_eligible);
    }

    #[test]
    fn failed_transition_does_not_erase_the_already_verified_root() {
        let scope = ScenarioProbeEvidenceScope::after_verified_root(false);
        assert!(scope.constructed_root_verified);
        assert!(!scope.original_game_transition_captured);
        assert!(!scope.simulator_comparison);
    }

    #[test]
    fn probe_case_compiler_binds_both_exact_drafts_before_launch() {
        let directory = std::env::temp_dir().join(format!("scenario-probe-{}", Uuid::new_v4()));
        let scenario_catalog = directory.join("scenarios");
        let probe_catalog = directory.join("probes");
        fs::create_dir_all(&scenario_catalog).unwrap();
        fs::create_dir_all(&probe_catalog).unwrap();
        let mut scenario_draft = super::super::tests::draft();
        scenario_draft.case_id = "test-cultist-root".to_owned();
        fs::write(
            scenario_catalog.join("test-cultist-root.json"),
            serde_json::to_vec(&scenario_draft).unwrap(),
        )
        .unwrap();
        fs::write(
            probe_catalog.join("test-cultist-end-turn.json"),
            serde_json::to_vec(&end_turn_draft()).unwrap(),
        )
        .unwrap();
        let scenario_plan_path = directory.join("scenario-plan.json");
        let probe_plan_path = directory.join("probe-plan.json");

        let plan = compile_scenario_probe_case(
            &probe_catalog,
            "test-cultist-end-turn",
            &scenario_catalog,
            &scenario_plan_path,
            &probe_plan_path,
        )
        .unwrap();
        assert_eq!(plan.probe_case_id, "test-cultist-end-turn");
        assert_eq!(plan.scenario_case_id, "test-cultist-root");
        assert_eq!(plan.transition, ScenarioProbeTransition::CombatEndTurn);
        assert_eq!(
            read_and_validate_probe_plan(&probe_plan_path).unwrap(),
            plan
        );

        let invalid_probe = ScenarioProbeDraft {
            schema: PROBE_DRAFT_SCHEMA.to_owned(),
            provenance: "constructed_validated".to_owned(),
            probe_case_id: "test-missing-second-strike".to_owned(),
            scenario_case_id: "test-cultist-root".to_owned(),
            transition: ScenarioProbeTransition::CombatPlayCard {
                card: ScenarioProbeCardSelector {
                    card_id: "Strike_R".to_owned(),
                    upgrades: 0,
                    occurrence: 1,
                },
                target: Some(ScenarioProbeMonsterSelector {
                    monster_id: "Cultist".to_owned(),
                    occurrence: 0,
                }),
            },
        };
        fs::write(
            probe_catalog.join("test-missing-second-strike.json"),
            serde_json::to_vec(&invalid_probe).unwrap(),
        )
        .unwrap();
        let rejected_scenario_plan = directory.join("rejected-scenario-plan.json");
        let rejected_probe_plan = directory.join("rejected-probe-plan.json");
        let error = compile_scenario_probe_case(
            &probe_catalog,
            "test-missing-second-strike",
            &scenario_catalog,
            &rejected_scenario_plan,
            &rejected_probe_plan,
        )
        .unwrap_err();
        assert!(error.contains("probe selects occurrence 1"), "{error}");
        assert!(!rejected_scenario_plan.exists());
        assert!(!rejected_probe_plan.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
