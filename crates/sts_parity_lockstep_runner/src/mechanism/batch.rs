use super::{replay_haste_mechanism_v1, HASTE_MECHANISM_SCOPE_V1};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::Duration,
};
use sts_parity_lockstep::{
    ComparisonStatusV1, LockstepModeV1, ProducerIdentityV1, SemanticActionV1,
};
use sts_parity_lockstep_java::{
    project_monster_ai_mechanism_v1, JavaLockstepTransportV1, JavaRuntimeObservationV1,
    OwnedJavaLockstepTransportV1, RecordedJavaActionPhaseV1, RecordedJavaActionV1,
    RecordedJavaBoundaryV1,
};

const TIMEOUT: Duration = Duration::from_secs(30);
const BATCH_SCHEMA: &str = "sts-shackled-haste-mechanism-batch-v1";
const CASES: [(u8, bool); 4] = [(0, false), (20, false), (0, true), (20, true)];

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BatchManifest {
    pub(super) schema: String,
    pub(super) scope: String,
    pub(super) producer: ProducerIdentityV1,
    pub(super) collector_executable_sha256: String,
    pub(super) files: BTreeMap<String, String>,
}

/// Collect an explicit, bounded matrix in an already-owned ScenarioControl
/// JVM. The caller owns launch/shutdown. On the first failure this returns
/// without resetting the case, advancing the matrix, or destroying the JVM.
/// Preparation is Java-only; it never acquires natural trajectory authority.
pub fn record_shackled_haste_batch_v1(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    mut progress: impl FnMut(&str),
) -> Result<(), String> {
    let initial = observe(oracle)?;
    if initial.frame["profile"] != "parity_scenario_control" {
        return Err(
            "mechanism collection requires an explicitly owned ScenarioControl oracle".to_owned(),
        );
    }
    fs::create_dir(destination).map_err(|error| error.to_string())?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut manifest = BatchManifest {
        schema: BATCH_SCHEMA.to_owned(),
        scope: HASTE_MECHANISM_SCOPE_V1.to_owned(),
        producer: initial.producer,
        collector_executable_sha256: file_hash(&executable)?,
        files: BTreeMap::new(),
    };
    put(&destination.join("capture.json"), &manifest)?;
    let launch: serde_json::Value = read(oracle.runtime().launch_plan())?;
    put(&destination.join("launch-plan.json"), &launch)?;
    for (index, (ascension, positive)) in CASES.into_iter().enumerate() {
        let name = case_name(ascension, positive);
        let result = record_case(
            oracle,
            destination,
            (ascension, positive),
            index > 0,
            &manifest.producer,
            &mut progress,
        );
        if let Err(reason) = result {
            put(
                &destination.join("stopped.json"),
                &json!({"case":name,"reason":reason,"scope":HASTE_MECHANISM_SCOPE_V1}),
            )?;
            return Err(format!("{name}: {reason}"));
        }
        for file in ["before.json", "action.json", "after.json"] {
            let relative = format!("{name}/{file}");
            manifest
                .files
                .insert(relative.clone(), file_hash(&destination.join(relative))?);
        }
        progress(&format!("{name} | Exact | monster + floor.ai"));
    }
    put(&destination.join("complete.json"), &manifest)?;
    Ok(())
}

/// Read-only replay of a sealed batch. No runtime discovery, Java connection,
/// cached comparison result, or version-specific derived snapshot is used.
pub fn replay_shackled_haste_batch_v1(directory: &Path) -> Result<usize, String> {
    let manifest: BatchManifest = read(&directory.join("complete.json"))?;
    if manifest.schema != BATCH_SCHEMA
        || manifest.scope != HASTE_MECHANISM_SCOPE_V1
        || manifest.files.len() != CASES.len() * 3
        || manifest.collector_executable_sha256.len() != 64
        || !manifest
            .collector_executable_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("unsupported or incomplete mechanism batch".to_owned());
    }
    for (ascension, positive) in CASES {
        let name = case_name(ascension, positive);
        for file in ["before.json", "action.json", "after.json"] {
            let relative = format!("{name}/{file}");
            let expected = manifest
                .files
                .get(&relative)
                .ok_or("mechanism batch omitted raw evidence")?;
            if &file_hash(&directory.join(relative))? != expected {
                return Err(format!("{name}: raw {file} bytes changed"));
            }
        }
        let case = directory.join(&name);
        let before: RecordedJavaBoundaryV1 = read(&case.join("before.json"))?;
        let after: RecordedJavaBoundaryV1 = read(&case.join("after.json"))?;
        let action: RecordedJavaActionV1 = read(&case.join("action.json"))?;
        if before.observation.producer != manifest.producer
            || after.observation.producer != manifest.producer
        {
            return Err(format!("{name}: batch producer changed"));
        }
        validate_case_input(&before.observation, ascension, positive)?;
        let report = replay_haste_mechanism_v1(&before, &action, &after)?;
        if report.status != ComparisonStatusV1::Exact {
            return Err(format!(
                "{name}: {}",
                report
                    .first_stop(LockstepModeV1::Audit)
                    .unwrap_or_else(|| "mechanism not Exact".to_owned())
            ));
        }
    }
    Ok(CASES.len())
}

fn record_case(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    case: (u8, bool),
    reset: bool,
    producer: &ProducerIdentityV1,
    progress: &mut impl FnMut(&str),
) -> Result<(), String> {
    let (ascension, positive) = case;
    let name = case_name(ascension, positive);
    let directory = destination.join(&name);
    fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let draft = json!({
        "schema":"sts_java_play_scenario_draft_v3", "provenance":"constructed_validated", "case_id":format!("shackled-haste-{name}"),
        "run":{"character":"IRONCLAD","ascension":ascension,"seed":"20260906","content_profile":"all_unlocked"},
        "recipe":{"recipe_id":"fresh_neow_to_combat_root.v1","capabilities":[
            {"capability_id":"combat.encounter.exact.v1","payload":{"encounter_id":"Time Eater"}},
            {"capability_id":"player.health.v1","payload":{"current_hp":999,"max_hp":999}},
            {"capability_id":"player.master_deck.exact.v1","payload":{"cards":[
                {"card_id":"Searing Blow","upgrades":15},{"card_id":"PiercingWail","upgrades":0},
                {"card_id":"Finesse","upgrades":0},{"card_id":"Escape Plan","upgrades":0}
            ]}},
            {"capability_id":"player.relics.fresh_exact.v1","payload":{"relics":[{"relic_id":"Runic Pyramid"}]}}
        ]}
    });
    create_case(oracle, &directory, &draft, reset, producer)?;
    let mut ordinal = 0;
    if positive {
        for cycle in 0..48 {
            // Analyst-authored preparation, never a strategy/search fallback.
            prepare_action(
                oracle,
                &directory,
                &mut ordinal,
                &["Finesse", "Escape Plan"],
            )?;
            if (cycle + 1) % 12 == 0 {
                progress(&format!(
                    "{name} preparation | {} Time Warp cycles",
                    (cycle + 1) / 12
                ));
            }
        }
    }
    // A bounded recipe: damage, then allow the original monster to select
    // Haste. Refuse an unexpected threshold result rather than improvise.
    for _ in 0..4 {
        let before = observe(oracle)?;
        if before.frame["parity_internal"]["monsters"][0]["next_move_id"] == 5 {
            break;
        }
        let monster = &before.frame["public_base"]["combat"]["monsters"][0];
        let hp = monster["current_hp"].as_i64().ok_or("monster HP missing")?;
        let max = monster["max_hp"].as_i64().ok_or("monster max HP missing")?;
        if hp < max / 2 {
            return Err("Haste not planned after threshold crossing".to_owned());
        }
        prepare_action(oracle, &directory, &mut ordinal, &["Searing Blow"])?;
        prepare_action(oracle, &directory, &mut ordinal, &[])?;
    }
    if observe(oracle)?.frame["parity_internal"]["monsters"][0]["next_move_id"] != 5 {
        return Err("bounded preparation did not reach Haste".to_owned());
    }
    prepare_action(oracle, &directory, &mut ordinal, &["PiercingWail"])?;
    let before = RecordedJavaBoundaryV1::new(observe(oracle)?)?;
    put(&directory.join("before.json"), &before)?;
    validate_case_input(&before.observation, ascension, positive)?;
    let actions = before
        .observation
        .actions
        .iter()
        .filter(|action| action.kind == "end_turn" && !action.parameters_required)
        .collect::<Vec<_>>();
    let [action] = actions.as_slice() else {
        return Err("mechanism requires one EndTurn action".to_owned());
    };
    let receipt = oracle
        .act(&action.runtime_handle)
        .map_err(|error| error.message)?;
    put(&directory.join("receipt.json"), &receipt)?;
    let action = RecordedJavaActionV1::new(
        RecordedJavaActionPhaseV1::Committed,
        SemanticActionV1::EndTurn,
        receipt,
    )?;
    put(&directory.join("action.json"), &action)?;
    let after = RecordedJavaBoundaryV1::new(observe(oracle)?)?;
    put(&directory.join("after.json"), &after)?;
    let comparison = replay_haste_mechanism_v1(&before, &action, &after)?;
    put(
        &directory.join("result.json"),
        &json!({"scope":HASTE_MECHANISM_SCOPE_V1,"preparation_actions":ordinal,"comparison":comparison}),
    )?;
    if comparison.status != ComparisonStatusV1::Exact {
        return Err(comparison
            .first_stop(LockstepModeV1::Audit)
            .unwrap_or_else(|| "mechanism not Exact".to_owned()));
    }
    Ok(())
}

/// Record raw evidence before invoking the same verifier used by offline tests.
pub(super) fn record_end_turn(
    oracle: &mut OwnedJavaLockstepTransportV1,
    directory: &Path,
    prefix: &str,
    validate: impl FnOnce(&RecordedJavaBoundaryV1) -> Result<(), String>,
    verify: impl FnOnce(
        &RecordedJavaBoundaryV1,
        &RecordedJavaActionV1,
        &RecordedJavaBoundaryV1,
    ) -> Result<sts_parity_lockstep::SnapshotComparisonV1, String>,
) -> Result<(), String> {
    let before = RecordedJavaBoundaryV1::new(observe(oracle)?)?;
    put(&directory.join(format!("{prefix}before.json")), &before)?;
    validate(&before)?;
    let actions = before
        .observation
        .actions
        .iter()
        .filter(|a| a.kind == "end_turn" && !a.parameters_required)
        .collect::<Vec<_>>();
    let [selected] = actions.as_slice() else {
        return Err("mechanism EndTurn missing or ambiguous".into());
    };
    let receipt = oracle
        .act(&selected.runtime_handle)
        .map_err(|e| e.message)?;
    put(&directory.join(format!("{prefix}receipt.json")), &receipt)?;
    let action = RecordedJavaActionV1::new(
        RecordedJavaActionPhaseV1::Committed,
        SemanticActionV1::EndTurn,
        receipt,
    )?;
    put(&directory.join(format!("{prefix}action.json")), &action)?;
    let after = RecordedJavaBoundaryV1::new(observe(oracle)?)?;
    put(&directory.join(format!("{prefix}after.json")), &after)?;
    let report = verify(&before, &action, &after)?;
    put(&directory.join(format!("{prefix}result.json")), &report)?;
    if report.status != ComparisonStatusV1::Exact {
        return Err(report
            .first_stop(LockstepModeV1::Audit)
            .unwrap_or_else(|| "mechanism not Exact".into()));
    }
    Ok(())
}

pub(super) fn create_case(
    oracle: &mut OwnedJavaLockstepTransportV1,
    directory: &Path,
    draft: &serde_json::Value,
    reset: bool,
    producer: &ProducerIdentityV1,
) -> Result<(), String> {
    put(&directory.join("draft.json"), draft)?;
    sts_java_parity_live::compile_scenario_plan(
        &directory.join("draft.json"),
        &directory.join("plan.json"),
    )?;
    if reset {
        observe(oracle)?;
        sts_java_parity_live::reset_scenario_to_main_menu(
            oracle.runtime().live_output(),
            &directory.join("reset.json"),
            TIMEOUT,
        )?;
    }
    let status = oracle
        .runtime_mut()
        .status()
        .map_err(|e| e.to_string())?
        .into_live();
    // The owner checked canonical identity; this API expects producer spelling.
    let plan = &status
        .manifest
        .launch
        .as_ref()
        .ok_or("oracle omitted launch evidence")?
        .plan_path;
    sts_java_parity_live::run_constructed_combat_scenario(
        oracle.runtime().live_output(),
        &directory.join("plan.json"),
        Path::new(plan),
        &directory.join("root-report.json"),
        TIMEOUT,
    )?;
    if observe(oracle)?.producer != *producer {
        return Err("oracle producer changed between cases".into());
    }
    Ok(())
}

fn validate_case_input(
    before: &JavaRuntimeObservationV1,
    ascension: u8,
    positive: bool,
) -> Result<(), String> {
    let (monsters, _) = project_monster_ai_mechanism_v1(before)?;
    let [monster] = monsters.as_slice() else {
        return Err("constructed case needs one monster".to_owned());
    };
    if before.frame["scenario"]["provenance"] != "constructed_validated"
        || before.frame["public_base"]["run"]["ascension"] != json!(ascension)
        || !monster.powers.iter().any(|power| {
            power.power_id == "strength" && power.amount == if positive { 2 } else { -6 }
        })
        || !monster
            .powers
            .iter()
            .any(|power| power.power_id == "shackled" && power.amount == 6)
    {
        return Err(
            "constructed case did not reach its declared ascension/Strength/Shackled input"
                .to_owned(),
        );
    }
    Ok(())
}

pub(super) fn prepare_action(
    oracle: &mut OwnedJavaLockstepTransportV1,
    directory: &Path,
    ordinal: &mut usize,
    card_priority: &[&str],
) -> Result<(), String> {
    prepare_action_with_upgrade(oracle, directory, ordinal, card_priority, None)
}

/// Preparation-only selection using observed Java upgrade metadata. The
/// selected action must still be unique; this creates no cross-runtime binding.
pub(super) fn prepare_action_with_upgrade(
    oracle: &mut OwnedJavaLockstepTransportV1,
    directory: &Path,
    ordinal: &mut usize,
    card_priority: &[&str],
    upgrades: Option<usize>,
) -> Result<(), String> {
    let before = RecordedJavaBoundaryV1::new(observe(oracle)?)?;
    let status = oracle
        .runtime_mut()
        .status()
        .map_err(|error| error.to_string())?
        .into_live();
    let card = card_priority.iter().find(|id| {
        status
            .actions
            .iter()
            .any(|action| action.kind == "play_card" && action.card_id.as_deref() == Some(**id))
    });
    if !card_priority.is_empty() && card.is_none() {
        return Err("preparation has no selected card action".to_owned());
    }
    let actions = status
        .actions
        .iter()
        .filter(|action| match card {
            Some(id) => {
                action.kind == "play_card"
                    && action.card_id.as_deref() == Some(*id)
                    && upgrades.is_none_or(|expected| {
                        observed_card_has_upgrade(
                            &before.observation.frame,
                            action.card_uuid.as_deref(),
                            action.card_id.as_deref(),
                            expected,
                        )
                    })
            }
            None => action.kind == "end_turn",
        })
        .collect::<Vec<_>>();
    let [action] = actions.as_slice() else {
        return Err("preparation action is missing or ambiguous".to_owned());
    };
    if status.frame["frame_id"] != json!(before.observation.frame_id) {
        return Err("preparation frame changed before selection".to_owned());
    }
    let prefix = format!("prep-{ordinal:03}");
    put(&directory.join(format!("{prefix}-before.json")), &before)?;
    put(
        &directory.join(format!("{prefix}-selection.json")),
        &json!({"frame_id":before.observation.frame_id,"selected":action,"scope":"Java-only fixture preparation; no cross-language card binding"}),
    )?;
    let receipt = oracle
        .runtime_mut()
        .act(&action.handle, TIMEOUT)
        .map_err(|error| error.to_string())?;
    put(&directory.join(format!("{prefix}-receipt.json")), &receipt)?;
    if receipt.status != "completed" {
        return Err("Java preparation action not completed".to_owned());
    }
    let after = RecordedJavaBoundaryV1::new(observe(oracle)?)?;
    put(&directory.join(format!("{prefix}-after.json")), &after)?;
    if receipt.based_on_frame_id != before.observation.frame_id
        || receipt.successor_frame_id != Some(after.observation.frame_id)
        || receipt.action_id != action.action_id
        || before.observation.producer != after.observation.producer
    {
        return Err("preparation receipt does not join its recorded boundaries".to_owned());
    }
    *ordinal += 1;
    Ok(())
}

pub(super) fn observe(
    oracle: &mut OwnedJavaLockstepTransportV1,
) -> Result<JavaRuntimeObservationV1, String> {
    oracle.status().map_err(|error| error.message)
}

fn observed_card_has_upgrade(
    frame: &serde_json::Value,
    uuid: Option<&str>,
    id: Option<&str>,
    upgrades: usize,
) -> bool {
    let Some(uuid) = uuid else {
        return false;
    };
    let Some(cards) = frame["parity_internal"]["zones"]["hand"]["cards"].as_array() else {
        return false;
    };
    let mut matching = cards
        .iter()
        .filter(|card| card["instance_uuid"].as_str() == Some(uuid));
    let Some(card) = matching.next() else {
        return false;
    };
    matching.next().is_none()
        && id.is_some()
        && card["card_id"].as_str() == id
        && card["upgrades"].as_u64() == Some(upgrades as u64)
}

fn case_name(ascension: u8, positive: bool) -> String {
    format!(
        "a{ascension}-{}",
        if positive { "positive" } else { "negative" }
    )
}

pub(super) fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

pub(super) fn file_hash(path: &Path) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(fs::read(path).map_err(|error| error.to_string())?)
    ))
}

pub(super) fn put(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec(value).map_err(|error| error.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_upgrade_selection_joins_unique_java_identity_not_hand_position() {
        let mut frame = json!({"parity_internal":{"zones":{"hand":{"cards":[
            {"instance_uuid":"upgraded", "card_id":"Combust", "upgrades":1},
            {"instance_uuid":"plain", "card_id":"Combust", "upgrades":0}
        ]}}}});
        assert!(observed_card_has_upgrade(
            &frame,
            Some("plain"),
            Some("Combust"),
            0
        ));
        assert!(observed_card_has_upgrade(
            &frame,
            Some("upgraded"),
            Some("Combust"),
            1
        ));
        assert!(!observed_card_has_upgrade(
            &frame,
            Some("plain"),
            Some("Combust"),
            1
        ));
        assert!(!observed_card_has_upgrade(&frame, None, Some("Combust"), 1));
        assert!(!observed_card_has_upgrade(
            &frame,
            Some("plain"),
            Some("Rupture"),
            0
        ));
        let cards = frame["parity_internal"]["zones"]["hand"]["cards"]
            .as_array_mut()
            .unwrap();
        cards.swap(0, 1);
        assert!(observed_card_has_upgrade(
            &frame,
            Some("plain"),
            Some("Combust"),
            0
        ));
        let cards = frame["parity_internal"]["zones"]["hand"]["cards"]
            .as_array_mut()
            .unwrap();
        cards.push(cards[0].clone());
        assert!(!observed_card_has_upgrade(
            &frame,
            Some("plain"),
            Some("Combust"),
            0
        ));
    }

    #[test]
    fn batch_replay_recomputes_results_and_checks_bytes_completeness_and_producer() {
        let directory =
            std::env::temp_dir().join(format!("sts-mechanism-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sts_parity_lockstep_java/tests/fixtures/haste_shackled");
        let sample: RecordedJavaBoundaryV1 =
            read(&fixtures.join("a0-negative-before.json")).unwrap();
        let mut manifest = BatchManifest {
            schema: BATCH_SCHEMA.to_owned(),
            scope: HASTE_MECHANISM_SCOPE_V1.to_owned(),
            producer: sample.observation.producer,
            collector_executable_sha256: "0".repeat(64),
            files: BTreeMap::new(),
        };
        // This is a synthetic storage-envelope test over retained oracle bytes,
        // never a newly collected live batch or provenance promotion.
        for (ascension, positive) in CASES {
            let name = case_name(ascension, positive);
            fs::create_dir(directory.join(&name)).unwrap();
            for kind in ["before", "action", "after"] {
                let relative = format!("{name}/{kind}.json");
                let target = directory.join(&relative);
                fs::copy(fixtures.join(format!("{name}-{kind}.json")), &target).unwrap();
                manifest.files.insert(relative, file_hash(&target).unwrap());
            }
        }
        assert!(replay_shackled_haste_batch_v1(&directory).is_err()); // not sealed
        put(&directory.join("complete.json"), &manifest).unwrap();
        assert_eq!(replay_shackled_haste_batch_v1(&directory).unwrap(), 4);
        let target = directory.join("a0-negative/after.json");
        let mut changed: RecordedJavaBoundaryV1 = read(&target).unwrap();
        changed.observation.frame["public_base"]["combat"]["monsters"][0]["block"] = json!(1);
        fs::write(&target, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(replay_shackled_haste_batch_v1(&directory)
            .unwrap_err()
            .contains("bytes changed"));
        manifest.files.insert(
            "a0-negative/after.json".to_owned(),
            file_hash(&target).unwrap(),
        );
        fs::write(
            directory.join("complete.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(replay_shackled_haste_batch_v1(&directory)
            .unwrap_err()
            .contains("$[0].block"));
        changed
            .observation
            .producer
            .session_epoch
            .push_str("-different");
        fs::write(&target, serde_json::to_vec(&changed).unwrap()).unwrap();
        manifest.files.insert(
            "a0-negative/after.json".to_owned(),
            file_hash(&target).unwrap(),
        );
        fs::write(
            directory.join("complete.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(replay_shackled_haste_batch_v1(&directory)
            .unwrap_err()
            .contains("producer changed"));
        // Only the uniquely created temporary test directory is removed.
        fs::remove_dir_all(&directory).unwrap();
    }
}
