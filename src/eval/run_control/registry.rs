use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::eval::artifact::ArtifactTrustLevel;
use crate::eval::combat_capture::load_combat_capture_v3;
use crate::eval::combat_search_v2::{
    validate_combat_search_v2_benchmark_schema_header, CombatSearchV2BenchmarkExpectedFingerprints,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenchmarkCasePaths {
    pub benchmark_manifest: PathBuf,
    pub capture_path: PathBuf,
    pub baseline_path: PathBuf,
}

impl BenchmarkCasePaths {
    pub fn for_case(root: &Path, case_id: &str) -> Self {
        Self {
            benchmark_manifest: root.join("benchmark.json"),
            capture_path: root
                .join("captures")
                .join(format!("{case_id}.capture.json")),
            baseline_path: root
                .join("baselines")
                .join(format!("{case_id}.baseline.json")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryManifest {
    schema_name: String,
    schema_version: u32,
    name: String,
    #[serde(default = "default_min_trust_level")]
    min_trust_level: ArtifactTrustLevel,
    cases: Vec<RegistryCase>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryCase {
    id: String,
    combat_snapshot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_fingerprints: Option<CombatSearchV2BenchmarkExpectedFingerprints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline: Option<String>,
}

pub fn add_case_to_benchmark_registry(
    root: &Path,
    case_id: &str,
) -> Result<BenchmarkCasePaths, String> {
    if case_id.trim().is_empty() {
        return Err("benchmark case id cannot be empty".to_string());
    }
    let paths = BenchmarkCasePaths::for_case(root, case_id);
    if !paths.capture_path.exists() {
        return Err(format!(
            "combat capture does not exist: {}",
            paths.capture_path.display()
        ));
    }
    let capture = load_combat_capture_v3(&paths.capture_path)?;

    let mut manifest = load_or_new_manifest(&paths.benchmark_manifest, root)?;
    let capture_rel = format!("captures/{case_id}.capture.json");
    let baseline = paths
        .baseline_path
        .exists()
        .then(|| format!("baselines/{case_id}.baseline.json"));
    let case = RegistryCase {
        id: case_id.to_string(),
        combat_snapshot: capture_rel,
        expected_fingerprints: Some(CombatSearchV2BenchmarkExpectedFingerprints {
            public_observation_hash: capture.fingerprints.public_observation_hash.clone(),
            legal_input_language_hash: capture.fingerprints.legal_input_language_hash.clone(),
            action_enumeration_domain_hash: capture
                .fingerprints
                .action_enumeration_domain_hash
                .clone(),
            exact_state_hash: Some(capture.fingerprints.exact_state_hash.clone()),
        }),
        baseline,
    };
    match manifest.cases.iter_mut().find(|case| case.id == case_id) {
        Some(existing) => *existing = case,
        None => manifest.cases.push(case),
    }
    manifest.cases.sort_by(|left, right| left.id.cmp(&right.id));

    if let Some(parent) = paths
        .benchmark_manifest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let payload = serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?;
    fs::write(&paths.benchmark_manifest, payload).map_err(|err| err.to_string())?;
    Ok(paths)
}

fn load_or_new_manifest(path: &Path, root: &Path) -> Result<RegistryManifest, String> {
    if path.exists() {
        let payload = fs::read_to_string(path).map_err(|err| err.to_string())?;
        validate_combat_search_v2_benchmark_schema_header(&payload)?;
        let manifest: RegistryManifest =
            serde_json::from_str(&payload).map_err(|err| err.to_string())?;
        if manifest.schema_name != default_benchmark_schema_name() {
            return Err(format!(
                "unsupported benchmark manifest schema '{}'",
                manifest.schema_name
            ));
        }
        if manifest.schema_version != default_benchmark_schema_version() {
            return Err(format!(
                "unsupported benchmark manifest schema_version {}",
                manifest.schema_version
            ));
        }
        if manifest.name.trim().is_empty() {
            return Err("benchmark manifest name cannot be empty".to_string());
        }
        return Ok(manifest);
    }

    Ok(RegistryManifest {
        schema_name: default_benchmark_schema_name(),
        schema_version: default_benchmark_schema_version(),
        name: root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("combat_search_v2_benchmark")
            .to_string(),
        min_trust_level: default_min_trust_level(),
        cases: Vec::new(),
    })
}

fn default_benchmark_schema_name() -> String {
    "CombatSearchV2BenchmarkSuiteV2".to_string()
}

fn default_benchmark_schema_version() -> u32 {
    2
}

fn default_min_trust_level() -> ArtifactTrustLevel {
    ArtifactTrustLevel::Restorable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::combat_capture::{capture_combat_position_v3, save_combat_capture_v3};
    use crate::fixtures::combat_start_spec::{compile_combat_start_spec, CombatStartSpec};
    use crate::sim::combat::CombatPosition;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn registry_adds_case_with_capture_fingerprints_without_requiring_baseline() {
        let root = unique_temp_dir("run_control_registry");
        let paths = BenchmarkCasePaths::for_case(&root, "case_a");
        let position = jaw_worm_position();
        let capture = capture_combat_position_v3(Some("case_a".to_string()), &position)
            .expect("capture should build");
        save_combat_capture_v3(&paths.capture_path, &capture).expect("capture should be saved");

        let written =
            add_case_to_benchmark_registry(&root, "case_a").expect("registry should update");
        let payload = fs::read_to_string(&written.benchmark_manifest).expect("manifest readable");

        assert!(payload.contains("\"schema_name\": \"CombatSearchV2BenchmarkSuiteV2\""));
        assert!(payload.contains("\"schema_version\": 2"));
        assert!(payload.contains("\"min_trust_level\": \"restorable\""));
        assert!(payload.contains("\"combat_snapshot\": \"captures/case_a.capture.json\""));
        assert!(payload.contains("\"expected_fingerprints\""));
        assert!(!payload.contains("\"baseline\""));
        crate::eval::combat_search_v2::load_combat_search_v2_benchmark(&written.benchmark_manifest)
            .expect("registry manifest should load as combat search benchmark");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn registry_adds_baseline_path_when_baseline_exists() {
        let root = unique_temp_dir("run_control_registry_baseline");
        let paths = BenchmarkCasePaths::for_case(&root, "case_a");
        let position = jaw_worm_position();
        let capture = capture_combat_position_v3(Some("case_a".to_string()), &position)
            .expect("capture should build");
        save_combat_capture_v3(&paths.capture_path, &capture).expect("capture should be saved");
        fs::create_dir_all(paths.baseline_path.parent().unwrap()).expect("baseline dir");
        fs::write(
            &paths.baseline_path,
            r#"{
                "schema_name": "CombatBaselineOutcomeV1",
                "schema_version": 1,
                "case_id": "case_a",
                "terminal": "win",
                "start_hp": 80,
                "final_hp": 70,
                "hp_loss": 10,
                "turns": 4,
                "potions_used": 0,
                "potions_discarded": 0,
                "cards_played": 9
            }"#,
        )
        .expect("baseline should be written");

        let written =
            add_case_to_benchmark_registry(&root, "case_a").expect("registry should update");
        let payload = fs::read_to_string(&written.benchmark_manifest).expect("manifest readable");

        assert!(payload.contains("\"baseline\": \"baselines/case_a.baseline.json\""));

        let _ = fs::remove_dir_all(root);
    }

    fn jaw_worm_position() -> CombatPosition {
        let spec: CombatStartSpec = serde_json::from_str(
            r#"{
                "name": "jaw_worm_starter",
                "player_class": "Ironclad",
                "ascension_level": 0,
                "encounter_id": "JawWorm",
                "room_type": "monster",
                "seed": 1,
                "player_current_hp": 80,
                "player_max_hp": 80,
                "master_deck": [
                    {"id": "Strike_R", "count": 5},
                    {"id": "Defend_R", "count": 4},
                    "Bash"
                ]
            }"#,
        )
        .expect("test start spec should parse");
        let (engine, combat) =
            compile_combat_start_spec(&spec).expect("test start spec should compile");
        CombatPosition::new(engine, combat)
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{label}_{}_{}", std::process::id(), nanos))
    }
}
