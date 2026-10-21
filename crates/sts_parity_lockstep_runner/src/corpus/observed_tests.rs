use super::*;
use sha2::{Digest, Sha256};

const CASE_ID: &str = "run-rng-probe-c5675540de994139931a40682abc5715";
const INITIAL: &str = "java-evidence/boundaries/00000000-frame-8.json";
const AFTER: &str = "java-evidence/boundaries/00000001-frame-9.json";
const ACTION: &str = "java-evidence/actions/00000000-from-frame-8.json";

fn fixture_files() -> Vec<(String, Vec<u8>)> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus_projection_drift");
    let sources: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.join("sources.json")).unwrap()).unwrap();
    let files = sources["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, source)| {
            let bytes = fs::read(fixture.join(name)).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                source["sha256"].as_str().unwrap(),
                "raw fixture bytes changed: {name}"
            );
            (name.clone(), bytes)
        })
        .collect::<Vec<_>>();
    let initial: RecordedJavaBoundaryV1 =
        serde_json::from_slice(&files.iter().find(|(name, _)| name == INITIAL).unwrap().1).unwrap();
    assert_eq!(
        serde_json::to_value(initial.observation.producer).unwrap(),
        sources["producer"]
    );
    files
}

fn copy_files(directory: &Path, files: &[(String, Vec<u8>)], include_manifest: bool) {
    for (name, bytes) in files {
        if !include_manifest && name == "case.json" {
            continue;
        }
        let path = directory.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

fn assert_unchanged(directory: &Path, files: &[(String, Vec<u8>)]) {
    for (name, bytes) in files {
        assert_eq!(
            &fs::read(directory.join(name)).unwrap(),
            bytes,
            "replay changed {name}"
        );
    }
}

#[test]
fn old_projection_annotations_replay_exact_from_unchanged_raw_boundaries() {
    let repository = tests::temporary_repository("observed-replay");
    let directory = repository
        .join(".oracle-lab/parity-corpus/cases")
        .join(CASE_ID);
    let files = fixture_files();
    copy_files(&directory, &files, true);
    let saved = load_manifest(&directory.join("case.json")).unwrap();
    let evidence = RecordedJavaEvidenceV1::load_complete(&directory.join("java-evidence")).unwrap();
    let current = manifest_from_evidence(&directory, saved.kind, &evidence).unwrap();
    assert_ne!(
        saved.coverage, current.coverage,
        "fixture must exercise projection drift"
    );

    let replay = replay_parity_corpus_v1(&repository).unwrap();
    assert_eq!(replay.cases, 1);
    assert_eq!(replay.committed_actions, 1);
    assert_eq!(replay.java_actions, 1);
    assert_unchanged(&directory, &files);
    fs::remove_dir_all(repository).unwrap();
}

#[test]
fn current_admission_deduplicates_against_reprojected_observed_case() {
    let repository = tests::temporary_repository("observed-dedup");
    let retained = repository
        .join(".oracle-lab/parity-corpus/cases")
        .join(CASE_ID);
    let files = fixture_files();
    copy_files(&retained, &files, true);
    // An explicitly isolated copy of the same raw source is the candidate;
    // this test does not claim a new Java recording or touch the local corpus.
    let staging = parity_corpus_staging_case_v1(&repository, "case-candidate").unwrap();
    copy_files(&staging, &files, false);
    let admission = admit_exact_parity_corpus_case_v1(
        &repository,
        &staging,
        ParityCorpusCaseKindV1::RunRngProbe,
    )
    .unwrap();
    assert_eq!(
        admission,
        ParityCorpusAdmissionV1::Duplicate {
            existing_case_id: CASE_ID.into()
        }
    );
    assert!(!staging.exists());
    assert_unchanged(&retained, &files);
    fs::remove_dir_all(repository).unwrap();
}

#[test]
fn observed_source_and_state_errors_still_stop_current_replay() {
    let files = fixture_files();
    let mutations: &[(&str, fn(&mut serde_json::Value), &str)] = &[
        (
            ACTION,
            |value| value["receipt"]["based_on_frame_id"] = 999.into(),
            "adjacent boundaries",
        ),
        (
            AFTER,
            |value| value["observation"]["producer"]["version"] = "changed".into(),
            "producer identity",
        ),
        (
            INITIAL,
            |value| value["observation"]["frame"]["public_base"]["run"]["gold"] = 100.into(),
            "gold",
        ),
        (
            INITIAL,
            |value| {
                value["observation"]["frame"]["public_base"]["run"]
                    .as_object_mut()
                    .unwrap()
                    .remove("gold");
            },
            "Java initial observation failed",
        ),
    ];
    for (file, mutate, expected) in mutations {
        let repository = tests::temporary_repository("observed-invalid");
        let directory = repository
            .join(".oracle-lab/parity-corpus/cases")
            .join(CASE_ID);
        copy_files(&directory, &files, true);
        let path = directory.join(file);
        let mut value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        mutate(&mut value);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = replay_parity_corpus_v1(&repository).unwrap_err();
        assert!(error.contains(expected), "{file}: {error}");
        assert!(directory.is_dir());
        fs::remove_dir_all(repository).unwrap();
    }
}
