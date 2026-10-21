use super::{
    batch::{
        create_case, file_hash, observe, prepare_action_with_upgrade, put, read, record_end_turn,
        BatchManifest,
    },
    hp_draw::build_input,
    replay_hp_draw_mechanism_v1, HpDrawCaseV1, HP_DRAW_MECHANISM_SCOPE_V1,
};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};
use sts_parity_lockstep::{ComparisonStatusV1, LockstepModeV1};
use sts_parity_lockstep_java::{
    OwnedJavaLockstepTransportV1, RecordedJavaActionV1, RecordedJavaBoundaryV1,
};

const SCHEMA: &str = "sts-hp-draw-mechanism-batch-v1";
const PREVENTION_SCHEMA: &str = "sts-hp-draw-prevention-mechanism-batch-v1";
const PREVENTION_SCOPE: &str = "Constructed A20 HP-loss EndTurn with Centennial Puzzle/Runic Cube and Anchor: creatures, ordered source-bound combat cards, relic state, AI/shuffle states and complete RNG calls; closed captured scenario only, no natural certification";

const POWERS_SCHEMA: &str = "sts-hp-draw-powers-mechanism-batch-v1";
const POWERS_SCOPE: &str = "Constructed A20 Rupture+ with Combust/Brutality and Centennial Puzzle/Runic Cube, optional Regret/Buffer/Tungsten Rod: creatures, source-bound cards, relics, AI/shuffle states and complete RNG calls; no natural certification";

pub fn record_hp_draw_powers_batch_v1(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    progress: impl FnMut(&str),
) -> Result<(), String> {
    record_batch(
        oracle,
        destination,
        POWERS_SCHEMA,
        POWERS_SCOPE,
        HpDrawCaseV1::POWERS,
        progress,
    )
}

pub fn record_hp_draw_prevention_batch_v1(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    progress: impl FnMut(&str),
) -> Result<(), String> {
    record_batch(
        oracle,
        destination,
        PREVENTION_SCHEMA,
        PREVENTION_SCOPE,
        HpDrawCaseV1::PREVENTION,
        progress,
    )
}

pub fn record_hp_draw_batch_v1(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    progress: impl FnMut(&str),
) -> Result<(), String> {
    record_batch(
        oracle,
        destination,
        SCHEMA,
        HP_DRAW_MECHANISM_SCOPE_V1,
        HpDrawCaseV1::ALL,
        progress,
    )
}

fn record_batch(
    oracle: &mut OwnedJavaLockstepTransportV1,
    destination: &Path,
    schema: &str,
    scope: &str,
    cases: &[HpDrawCaseV1],
    mut progress: impl FnMut(&str),
) -> Result<(), String> {
    let initial = observe(oracle)?;
    if initial.frame["profile"] != "parity_scenario_control" {
        return Err("HP draw requires ScenarioControl".into());
    }
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    let mut manifest = BatchManifest {
        schema: schema.into(),
        scope: scope.into(),
        producer: initial.producer,
        collector_executable_sha256: file_hash(
            &std::env::current_exe().map_err(|e| e.to_string())?,
        )?,
        files: BTreeMap::new(),
    };
    put(&destination.join("capture.json"), &manifest)?;
    let launch: serde_json::Value = read(oracle.runtime().launch_plan())?;
    put(&destination.join("launch-plan.json"), &launch)?;
    for (index, &case) in cases.iter().enumerate() {
        let name = case.name();
        let directory = destination.join(&name);
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let result = (|| {
            create_case(
                oracle,
                &directory,
                &case.draft(),
                index > 0,
                &manifest.producer,
            )?;
            let mut ordinal = 0;
            for (id, upgrades) in case.preparations() {
                prepare_action_with_upgrade(
                    oracle,
                    &directory,
                    &mut ordinal,
                    &[id.stable_id()],
                    Some(usize::from(upgrades)),
                )?;
            }
            record_end_turn(
                oracle,
                &directory,
                "",
                |before| {
                    let (_, recipe) = build_input(case, before)?;
                    put(&directory.join("captured-recipe.json"), &recipe)
                },
                |b, a, z| replay_hp_draw_mechanism_v1(case, b, a, z),
            )
        })();
        if let Err(reason) = result {
            put(
                &destination.join("stopped.json"),
                &json!({"case":name,"reason":reason,"scope":scope}),
            )?;
            return Err(format!("{name}: {reason}"));
        }
        for kind in ["before", "action", "after"] {
            let relative = format!("{name}/{kind}.json");
            manifest
                .files
                .insert(relative.clone(), file_hash(&destination.join(relative))?);
        }
        progress(&format!(
            "{name} | Exact | creatures, bound cards, relics + RNG"
        ));
    }
    put(&destination.join("complete.json"), &manifest)
}

pub fn replay_hp_draw_batch_v1(directory: &Path) -> Result<usize, String> {
    let manifest: BatchManifest = read(&directory.join("complete.json"))?;
    let cases = match (manifest.schema.as_str(), manifest.scope.as_str()) {
        (SCHEMA, HP_DRAW_MECHANISM_SCOPE_V1) => HpDrawCaseV1::ALL,
        (PREVENTION_SCHEMA, PREVENTION_SCOPE) => HpDrawCaseV1::PREVENTION,
        (POWERS_SCHEMA, POWERS_SCOPE) => HpDrawCaseV1::POWERS,
        _ => return Err("HP draw batch has an unsupported schema or scope".into()),
    };
    if manifest.files.len() != cases.len() * 3
        || manifest.collector_executable_sha256.len() != 64
        || !manifest
            .collector_executable_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err("HP draw batch is incomplete or unsupported".into());
    }
    for &case in cases {
        let name = case.name();
        let dir = directory.join(&name);
        for kind in ["before", "action", "after"] {
            let relative = format!("{name}/{kind}.json");
            if manifest.files.get(&relative) != Some(&file_hash(&directory.join(&relative))?) {
                return Err(format!("{relative}: raw bytes changed or missing"));
            }
        }
        let before: RecordedJavaBoundaryV1 = read(&dir.join("before.json"))?;
        let action: RecordedJavaActionV1 = read(&dir.join("action.json"))?;
        let after: RecordedJavaBoundaryV1 = read(&dir.join("after.json"))?;
        if before.observation.producer != manifest.producer
            || after.observation.producer != manifest.producer
        {
            return Err("HP draw batch producer changed".into());
        }
        let report = replay_hp_draw_mechanism_v1(case, &before, &action, &after)?;
        if report.status != ComparisonStatusV1::Exact {
            return Err(format!(
                "{name}: {}",
                report
                    .first_stop(LockstepModeV1::Audit)
                    .unwrap_or_else(|| "not Exact".into())
            ));
        }
    }
    Ok(cases.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recorded_hp_draw_batch_requires_complete_bytes_and_the_original_producer() {
        assert_batch_contract("hp_loss_draw", 0, 11);
        assert_batch_contract("hp_loss_draw", 1, 2);
    }

    #[test]
    fn recorded_hp_draw_powers_batch_requires_complete_bytes_and_the_original_producer() {
        assert_batch_contract("hp_loss_draw_powers", 0, 18);
    }

    fn assert_batch_contract(fixture_name: &str, batch_index: usize, case_count: usize) {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sts_parity_lockstep_java/tests/fixtures")
            .join(fixture_name);
        let sources: serde_json::Value = read(&fixture.join("sources.json")).unwrap();
        let manifest: BatchManifest =
            serde_json::from_value(sources["batches"][batch_index]["complete_manifest"].clone())
                .unwrap();
        let directory =
            std::env::temp_dir().join(format!("hp-draw-batch-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        assert!(replay_hp_draw_batch_v1(&directory).is_err());
        for relative in manifest.files.keys() {
            let destination = directory.join(relative);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(fixture.join(relative), &destination).unwrap();
        }
        let complete = directory.join("complete.json");
        put(&complete, &manifest).unwrap();
        assert_eq!(replay_hp_draw_batch_v1(&directory).unwrap(), case_count);
        let mut changed = serde_json::to_value(&manifest).unwrap();
        changed["producer"]["session_epoch"] = json!("different-producer");
        fs::write(&complete, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(replay_hp_draw_batch_v1(&directory)
            .unwrap_err()
            .contains("producer"));
        fs::write(&complete, serde_json::to_vec(&manifest).unwrap()).unwrap();
        // The same decoded JSON with altered raw bytes must also fail.
        let relative = manifest.files.keys().next().unwrap();
        let target = directory.join(relative);
        let mut bytes = fs::read(&target).unwrap();
        bytes.push(b' ');
        fs::write(&target, bytes).unwrap();
        assert!(replay_hp_draw_batch_v1(&directory)
            .unwrap_err()
            .contains("bytes changed"));
        assert!(
            directory.is_absolute() && directory.parent() == Some(std::env::temp_dir().as_path())
        );
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn hp_draw_recipes_compile_without_java() {
        let directory =
            std::env::temp_dir().join(format!("hp-draw-recipe-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        for &case in HpDrawCaseV1::ALL
            .iter()
            .chain(HpDrawCaseV1::PREVENTION)
            .chain(HpDrawCaseV1::POWERS)
        {
            let draft = directory.join(format!("{}.json", case.name()));
            put(&draft, &case.draft()).unwrap();
            sts_java_parity_live::compile_scenario_plan(
                &draft,
                &directory.join(format!("{}-plan.json", case.name())),
            )
            .unwrap();
        }
        assert!(
            directory.is_absolute() && directory.parent() == Some(std::env::temp_dir().as_path())
        );
        fs::remove_dir_all(&directory).unwrap();
    }
}
