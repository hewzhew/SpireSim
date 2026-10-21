use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use sts_parity_lockstep::{
    DomainEvidenceV1, NaturalRunContentProfileV1, NaturalRunNeowEligibilityV1, ParityDomainV1,
    SemanticActionV1,
};
use sts_parity_lockstep_java::{RecordedJavaBoundaryV1, RecordedJavaEvidenceV1};

pub const PARITY_CORPUS_CASE_SCHEMA_V1: &str = "sts-parity-corpus-case-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParityCorpusCaseKindV1 {
    RunRngProbe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParityCorpusAdmissionV1 {
    Admitted { case_id: String },
    Duplicate { existing_case_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParityCorpusReplayReportV1 {
    pub cases: usize,
    pub committed_actions: usize,
    pub java_actions: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ParityCorpusCaseManifestV1 {
    schema: String,
    case_id: String,
    kind: ParityCorpusCaseKindV1,
    producer_implementation: String,
    producer_version: String,
    /// Coverage derived at admission. Boundary kinds and domain schemas are
    /// historical annotations, not the current replay contract or dedup key.
    coverage: ParityCorpusCoverageV1,
    committed_actions: usize,
    java_actions: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ParityCorpusCoverageV1 {
    player_class: String,
    ascension: u8,
    final_act: bool,
    content_profile: NaturalRunContentProfileV1,
    neow_eligibility: NaturalRunNeowEligibilityV1,
    initial: ParityCorpusBoundaryCoverageV1,
    transitions: Vec<ParityCorpusTransitionCoverageV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ParityCorpusTransitionCoverageV1 {
    before: ParityCorpusBoundaryCoverageV1,
    action: SemanticActionV1,
    after: ParityCorpusBoundaryCoverageV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ParityCorpusBoundaryCoverageV1 {
    boundary_kind: String,
    domains: Vec<ParityCorpusDomainCoverageV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ParityCorpusDomainCoverageV1 {
    domain: ParityDomainV1,
    observed_schema: Option<String>,
}

pub fn parity_corpus_staging_case_v1(repository: &Path, case_id: &str) -> Result<PathBuf, String> {
    validate_case_id(case_id)?;
    let repository = canonical_directory(repository, "repository")?;
    let staging = repository
        .join(".oracle-lab")
        .join("parity-corpus")
        .join("staging");
    fs::create_dir_all(&staging)
        .map_err(|error| format!("failed to create parity corpus staging: {error}"))?;
    Ok(staging.join(case_id))
}

/// Admit one complete exact case and remove only a newly generated staging
/// directory when the same typed coverage is already retained.
pub fn admit_exact_parity_corpus_case_v1(
    repository: &Path,
    staging_case: &Path,
    kind: ParityCorpusCaseKindV1,
) -> Result<ParityCorpusAdmissionV1, String> {
    let evidence_directory = staging_case.join("java-evidence");
    crate::replay_recorded_evidence_v1(&evidence_directory)?;
    let evidence = RecordedJavaEvidenceV1::load_complete(&evidence_directory)?;
    let manifest = manifest_from_evidence(staging_case, kind, &evidence)?;
    admit_manifest(repository, staging_case, manifest)
}

/// Replay every retained case with the current projection and Rust simulator.
///
/// Staging candidates are deliberately ignored. The first invalid or
/// non-exact retained case stops the replay without launching or discovering
/// Java.
pub fn replay_parity_corpus_v1(repository: &Path) -> Result<ParityCorpusReplayReportV1, String> {
    let repository = canonical_directory(repository, "repository")?;
    let cases = repository
        .join(".oracle-lab")
        .join("parity-corpus")
        .join("cases");
    if !cases.is_dir() {
        return Err("parity corpus has no retained cases".to_owned());
    }
    let mut directories = fs::read_dir(&cases)
        .map_err(|error| format!("failed to read parity corpus cases: {error}"))?
        .map(|entry| {
            let entry =
                entry.map_err(|error| format!("failed to read parity corpus case: {error}"))?;
            entry
                .file_type()
                .map_err(|error| format!("failed to inspect parity corpus case: {error}"))?
                .is_dir()
                .then_some(entry.path())
                .ok_or_else(|| "parity corpus cases contains a non-directory entry".to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    directories.sort();
    if directories.is_empty() {
        return Err("parity corpus has no retained cases".to_owned());
    }

    let mut report = ParityCorpusReplayReportV1 {
        cases: 0,
        committed_actions: 0,
        java_actions: 0,
    };
    for (ordinal, directory) in directories.into_iter().enumerate() {
        let manifest = load_manifest(&directory.join("case.json"))?;
        let directory_id = directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "parity corpus case directory has no UTF-8 identity".to_owned())?;
        if directory_id != manifest.case_id {
            return Err(format!(
                "parity corpus case {ordinal} directory identity does not match its manifest"
            ));
        }
        let evidence_directory = directory.join("java-evidence");
        let evidence = RecordedJavaEvidenceV1::load_complete(&evidence_directory)
            .map_err(|error| format!("parity corpus case {ordinal} is incomplete: {error}"))?;
        validate_manifest_source(&directory, &manifest, &evidence)
            .map_err(|error| format!("parity corpus case {ordinal}: {error}"))?;
        let replay = crate::replay_recorded_evidence_v1(&evidence_directory)
            .map_err(|error| format!("parity corpus case {ordinal} stopped: {error}"))?;
        report.cases += 1;
        report.committed_actions += replay.committed_actions;
        report.java_actions += replay.java_actions;
    }
    Ok(report)
}

/// Validates facts carried by the raw recording, without treating a saved
/// projection as authority. Current projection validity and Exactness are
/// checked separately by the shared offline replay.
fn validate_manifest_source(
    directory: &Path,
    manifest: &ParityCorpusCaseManifestV1,
    evidence: &RecordedJavaEvidenceV1,
) -> Result<(), String> {
    let bootstrap = &evidence.manifest.bootstrap;
    let producer = &evidence.initial.observation.producer;
    let source_matches = [
        (
            "case_id",
            directory.file_name().and_then(|name| name.to_str()) == Some(manifest.case_id.as_str()),
        ),
        (
            "producer_implementation",
            manifest.producer_implementation == producer.implementation,
        ),
        (
            "producer_version",
            manifest.producer_version == producer.version,
        ),
        (
            "coverage.player_class",
            manifest.coverage.player_class == bootstrap.player_class,
        ),
        (
            "coverage.ascension",
            manifest.coverage.ascension == bootstrap.ascension,
        ),
        (
            "coverage.final_act",
            manifest.coverage.final_act == bootstrap.final_act,
        ),
        (
            "coverage.content_profile",
            manifest.coverage.content_profile == bootstrap.content_profile,
        ),
        (
            "coverage.neow_eligibility",
            manifest.coverage.neow_eligibility == bootstrap.neow_eligibility,
        ),
        (
            "committed_actions",
            manifest.committed_actions == evidence.steps.len(),
        ),
        (
            "java_actions",
            manifest.java_actions == evidence.java_action_count,
        ),
        (
            "coverage.transitions.len",
            manifest.coverage.transitions.len() == evidence.steps.len(),
        ),
    ];
    if let Some((field, _)) = source_matches.into_iter().find(|(_, matches)| !matches) {
        return Err(format!("recorded corpus source differs at {field}"));
    }
    for (index, (declared, recorded)) in manifest
        .coverage
        .transitions
        .iter()
        .zip(&evidence.steps)
        .enumerate()
    {
        if declared.action != recorded.action {
            return Err(format!(
                "recorded corpus source differs at coverage.transitions[{index}].action"
            ));
        }
    }
    Ok(())
}

fn manifest_from_evidence(
    staging_case: &Path,
    kind: ParityCorpusCaseKindV1,
    evidence: &RecordedJavaEvidenceV1,
) -> Result<ParityCorpusCaseManifestV1, String> {
    let case_id = staging_case
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "parity corpus staging case has no UTF-8 identity".to_owned())?
        .to_owned();
    validate_case_id(&case_id)?;
    let bootstrap = &evidence.manifest.bootstrap;
    let producer = &evidence.initial.observation.producer;
    let transitions = evidence
        .steps
        .iter()
        .map(|step| {
            Ok(ParityCorpusTransitionCoverageV1 {
                before: boundary_coverage(&step.before, bootstrap)?,
                action: step.action.clone(),
                after: boundary_coverage(&step.after, bootstrap)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ParityCorpusCaseManifestV1 {
        schema: PARITY_CORPUS_CASE_SCHEMA_V1.to_owned(),
        case_id,
        kind,
        producer_implementation: producer.implementation.clone(),
        producer_version: producer.version.clone(),
        coverage: ParityCorpusCoverageV1 {
            player_class: bootstrap.player_class.clone(),
            ascension: bootstrap.ascension,
            final_act: bootstrap.final_act,
            content_profile: bootstrap.content_profile,
            neow_eligibility: bootstrap.neow_eligibility,
            initial: boundary_coverage(&evidence.initial, bootstrap)?,
            transitions,
        },
        committed_actions: evidence.steps.len(),
        java_actions: evidence.java_action_count,
    })
}

fn boundary_coverage(
    boundary: &RecordedJavaBoundaryV1,
    bootstrap: &sts_parity_lockstep::NaturalRunBootstrapV1,
) -> Result<ParityCorpusBoundaryCoverageV1, String> {
    let snapshot = boundary.reproject(bootstrap)?;
    let domains = snapshot
        .domains
        .into_iter()
        .map(|(domain, observation)| ParityCorpusDomainCoverageV1 {
            domain,
            observed_schema: match observation.evidence {
                DomainEvidenceV1::Observed { schema, .. } => Some(schema),
                DomainEvidenceV1::Unobserved { .. } => None,
            },
        })
        .collect();
    Ok(ParityCorpusBoundaryCoverageV1 {
        boundary_kind: snapshot.boundary_kind,
        domains,
    })
}

fn admit_manifest(
    repository: &Path,
    staging_case: &Path,
    manifest: ParityCorpusCaseManifestV1,
) -> Result<ParityCorpusAdmissionV1, String> {
    admit_manifest_with_reviewer(
        repository,
        staging_case,
        manifest,
        |directory, retained, candidate| {
            let evidence_directory = directory.join("java-evidence");
            let evidence = RecordedJavaEvidenceV1::load_complete(&evidence_directory)?;
            validate_manifest_source(directory, retained, &evidence)?;
            let current = manifest_from_evidence(directory, retained.kind, &evidence)?;
            if current.kind != candidate.kind || current.coverage != candidate.coverage {
                return Ok(RetainedCaseReviewV1::DifferentCoverage);
            }
            Ok(RetainedCaseReviewV1::SameCoverage {
                replay_exact: crate::replay_recorded_evidence_v1(&evidence_directory).is_ok(),
            })
        },
    )
}

enum RetainedCaseReviewV1 {
    DifferentCoverage,
    SameCoverage { replay_exact: bool },
}

fn admit_manifest_with_reviewer(
    repository: &Path,
    staging_case: &Path,
    manifest: ParityCorpusCaseManifestV1,
    review_retained: impl Fn(
        &Path,
        &ParityCorpusCaseManifestV1,
        &ParityCorpusCaseManifestV1,
    ) -> Result<RetainedCaseReviewV1, String>,
) -> Result<ParityCorpusAdmissionV1, String> {
    let repository = canonical_directory(repository, "repository")?;
    let corpus = repository.join(".oracle-lab").join("parity-corpus");
    let staging = corpus.join("staging");
    let cases = corpus.join("cases");
    fs::create_dir_all(&staging)
        .map_err(|error| format!("failed to create parity corpus staging: {error}"))?;
    fs::create_dir_all(&cases)
        .map_err(|error| format!("failed to create parity corpus cases: {error}"))?;
    let staging = fs::canonicalize(&staging)
        .map_err(|error| format!("failed to resolve parity corpus staging: {error}"))?;
    let source = fs::canonicalize(staging_case)
        .map_err(|error| format!("failed to resolve parity corpus candidate: {error}"))?;
    if source.parent() != Some(staging.as_path()) || !source.is_dir() {
        return Err("parity corpus candidate is outside its managed staging directory".to_owned());
    }

    let mut duplicate = None;
    let mut stale_duplicates = Vec::new();
    for entry in fs::read_dir(&cases)
        .map_err(|error| format!("failed to read parity corpus cases: {error}"))?
    {
        let entry = entry.map_err(|error| format!("failed to read parity corpus case: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("failed to inspect parity corpus case: {error}"))?
            .is_dir()
        {
            return Err("parity corpus cases contains a non-directory entry".to_owned());
        }
        let retained = load_manifest(&entry.path().join("case.json"))?;
        // An unreadable source or failed projection has unknown current
        // coverage: preserve both records and stop admission, never delete it
        // merely because its saved coverage annotation resembles the candidate.
        match review_retained(&entry.path(), &retained, &manifest)? {
            RetainedCaseReviewV1::DifferentCoverage => {}
            RetainedCaseReviewV1::SameCoverage { replay_exact: true } => {
                duplicate.get_or_insert(retained.case_id);
            }
            RetainedCaseReviewV1::SameCoverage {
                replay_exact: false,
            } => stale_duplicates.push(entry.path()),
        }
    }

    if let Some(existing_case_id) = duplicate {
        remove_stale_duplicates(&stale_duplicates)?;
        fs::remove_dir_all(&source)
            .map_err(|error| format!("failed to discard duplicate corpus staging: {error}"))?;
        return Ok(ParityCorpusAdmissionV1::Duplicate { existing_case_id });
    }

    write_manifest_idempotent(&source.join("case.json"), &manifest)?;
    let destination = cases.join(&manifest.case_id);
    if destination.exists() {
        return Err("parity corpus destination already exists".to_owned());
    }
    fs::rename(&source, &destination)
        .map_err(|error| format!("failed to promote parity corpus case: {error}"))?;
    remove_stale_duplicates(&stale_duplicates)?;
    Ok(ParityCorpusAdmissionV1::Admitted {
        case_id: manifest.case_id,
    })
}

fn remove_stale_duplicates(paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        fs::remove_dir_all(path)
            .map_err(|error| format!("failed to retire stale duplicate corpus case: {error}"))?;
    }
    Ok(())
}

fn load_manifest(path: &Path) -> Result<ParityCorpusCaseManifestV1, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read parity corpus manifest: {error}"))?;
    let manifest: ParityCorpusCaseManifestV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode parity corpus manifest: {error}"))?;
    if manifest.schema != PARITY_CORPUS_CASE_SCHEMA_V1 {
        return Err("parity corpus manifest has an unsupported schema".to_owned());
    }
    validate_case_id(&manifest.case_id)?;
    Ok(manifest)
}

fn write_manifest_idempotent(
    path: &Path,
    manifest: &ParityCorpusCaseManifestV1,
) -> Result<(), String> {
    if path.exists() {
        return (load_manifest(path)? == *manifest)
            .then_some(())
            .ok_or_else(|| "parity corpus staging manifest changed between retries".to_owned());
    }
    let bytes = serde_json::to_vec(manifest)
        .map_err(|error| format!("failed to encode parity corpus manifest: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create parity corpus manifest: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist parity corpus manifest: {error}"))
}

fn validate_case_id(case_id: &str) -> Result<(), String> {
    (!case_id.is_empty()
        && case_id.len() <= 96
        && case_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
    .then_some(())
    .ok_or_else(|| "parity corpus case identity is invalid".to_owned())
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let path =
        fs::canonicalize(path).map_err(|error| format!("failed to resolve {label}: {error}"))?;
    path.is_dir()
        .then_some(path)
        .ok_or_else(|| format!("{label} is not a directory"))
}

#[cfg(test)]
#[path = "corpus/observed_tests.rs"]
mod observed_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use sts_parity_lockstep::NaturalRunContentProfileV1;

    #[test]
    fn identical_typed_coverage_keeps_only_the_first_case() {
        let repository = temporary_repository("dedup");
        let first = staging_case(&repository, "case-first");
        let second = staging_case(&repository, "case-second");
        fs::write(first.join("payload"), b"first").unwrap();
        fs::write(second.join("payload"), b"second").unwrap();

        let first_result = admit_manifest_with_reviewer(
            &repository,
            &first,
            test_manifest("case-first", test_coverage()),
            |_, _, _| Ok(RetainedCaseReviewV1::SameCoverage { replay_exact: true }),
        )
        .unwrap();
        let second_result = admit_manifest_with_reviewer(
            &repository,
            &second,
            test_manifest("case-second", test_coverage()),
            |_, _, _| Ok(RetainedCaseReviewV1::SameCoverage { replay_exact: true }),
        )
        .unwrap();

        assert!(matches!(
            first_result,
            ParityCorpusAdmissionV1::Admitted { .. }
        ));
        assert_eq!(
            second_result,
            ParityCorpusAdmissionV1::Duplicate {
                existing_case_id: "case-first".to_owned()
            }
        );
        assert!(!second.exists());
        assert!(repository
            .join(".oracle-lab/parity-corpus/cases/case-first/payload")
            .is_file());
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn current_exact_candidate_replaces_a_stale_duplicate() {
        let repository = temporary_repository("replace-stale");
        let stale = staging_case(&repository, "case-stale");
        let current = staging_case(&repository, "case-current");
        fs::write(stale.join("payload"), b"stale").unwrap();
        fs::write(current.join("payload"), b"current").unwrap();

        admit_manifest_with_reviewer(
            &repository,
            &stale,
            test_manifest("case-stale", test_coverage()),
            |_, _, _| Ok(RetainedCaseReviewV1::SameCoverage { replay_exact: true }),
        )
        .unwrap();
        let result = admit_manifest_with_reviewer(
            &repository,
            &current,
            test_manifest("case-current", test_coverage()),
            |_, _, _| {
                Ok(RetainedCaseReviewV1::SameCoverage {
                    replay_exact: false,
                })
            },
        )
        .unwrap();

        assert!(matches!(result, ParityCorpusAdmissionV1::Admitted { .. }));
        let cases = repository.join(".oracle-lab/parity-corpus/cases");
        assert!(!cases.join("case-stale").exists());
        assert_eq!(
            fs::read(cases.join("case-current/payload")).unwrap(),
            b"current"
        );
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn dedup_uses_current_coverage_without_rewriting_recorded_annotations() {
        let repository = temporary_repository("projection-drift");
        let retained = repository.join(".oracle-lab/parity-corpus/cases/case-retained");
        fs::create_dir_all(&retained).unwrap();
        let stored = test_manifest("case-retained", test_coverage());
        write_manifest_idempotent(&retained.join("case.json"), &stored).unwrap();
        fs::write(retained.join("raw-payload"), b"unchanged source").unwrap();
        let original = fs::read(retained.join("case.json")).unwrap();
        let staging = staging_case(&repository, "case-candidate");
        let mut current = test_coverage();
        current.initial.domains[0].observed_schema = Some("run-current".into());

        let result = admit_manifest_with_reviewer(
            &repository,
            &staging,
            test_manifest("case-candidate", current),
            |_, saved, candidate| {
                assert_ne!(saved.coverage, candidate.coverage);
                Ok(RetainedCaseReviewV1::SameCoverage { replay_exact: true })
            },
        )
        .unwrap();

        assert!(matches!(result, ParityCorpusAdmissionV1::Duplicate { .. }));
        assert!(!staging.exists());
        assert_eq!(fs::read(retained.join("case.json")).unwrap(), original);
        assert_eq!(
            fs::read(retained.join("raw-payload")).unwrap(),
            b"unchanged source"
        );
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn matching_saved_annotations_cannot_override_newly_different_coverage() {
        let repository = temporary_repository("changed-coverage");
        let retained = repository.join(".oracle-lab/parity-corpus/cases/case-retained");
        fs::create_dir_all(&retained).unwrap();
        write_manifest_idempotent(
            &retained.join("case.json"),
            &test_manifest("case-retained", test_coverage()),
        )
        .unwrap();
        let staging = staging_case(&repository, "case-candidate");
        let result = admit_manifest_with_reviewer(
            &repository,
            &staging,
            test_manifest("case-candidate", test_coverage()),
            |_, saved, candidate| {
                assert_eq!(saved.coverage, candidate.coverage);
                Ok(RetainedCaseReviewV1::DifferentCoverage)
            },
        )
        .unwrap();
        assert!(matches!(result, ParityCorpusAdmissionV1::Admitted { .. }));
        assert!(retained.is_dir());
        assert!(repository
            .join(".oracle-lab/parity-corpus/cases/case-candidate")
            .is_dir());
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn failed_source_or_projection_review_preserves_both_records() {
        let repository = temporary_repository("unreviewable");
        let retained = repository.join(".oracle-lab/parity-corpus/cases/case-retained");
        fs::create_dir_all(&retained).unwrap();
        write_manifest_idempotent(
            &retained.join("case.json"),
            &test_manifest("case-retained", test_coverage()),
        )
        .unwrap();
        let staging = staging_case(&repository, "case-candidate");
        let result = admit_manifest_with_reviewer(
            &repository,
            &staging,
            test_manifest("case-candidate", test_coverage()),
            |_, _, _| Err("current coverage is unknown".into()),
        );
        assert!(result.is_err());
        assert!(retained.is_dir());
        assert!(staging.is_dir());
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn recorded_source_check_preserves_actions_and_provenance_but_not_projection_versions() {
        let repository = temporary_repository("source-contract");
        let directory = staging_case(&repository, "case-source");
        let evidence = write_test_source(&directory);
        let manifest = test_manifest("case-source", test_coverage());
        validate_manifest_source(&directory, &manifest, &evidence).unwrap();
        let mut old_projection = manifest.clone();
        old_projection.coverage.initial.domains.clear();
        old_projection.coverage.transitions[0].after.boundary_kind = "old-derived-kind".into();
        validate_manifest_source(&directory, &old_projection, &evidence).unwrap();

        let corruptions: &[fn(&mut ParityCorpusCaseManifestV1)] = &[
            |m| m.case_id.push_str("-wrong"),
            |m| m.producer_implementation.push_str("-wrong"),
            |m| m.producer_version.push_str("-wrong"),
            |m| m.coverage.player_class = "SILENT".into(),
            |m| m.coverage.ascension = 0,
            |m| m.coverage.final_act = false,
            |m| m.coverage.neow_eligibility = NaturalRunNeowEligibilityV1::Mini,
            |m| m.committed_actions += 1,
            |m| m.java_actions += 1,
            |m| m.coverage.transitions.clear(),
            |m| {
                m.coverage.transitions[0].action = SemanticActionV1::Proceed {
                    surface: sts_parity_lockstep::DecisionSurfaceV1::Map,
                }
            },
        ];
        for corrupt in corruptions {
            let mut changed = manifest.clone();
            corrupt(&mut changed);
            assert!(validate_manifest_source(&directory, &changed, &evidence).is_err());
        }
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn saved_coverage_does_not_authorize_replay_of_unobserved_raw_state() {
        let repository = temporary_repository("unobserved-replay");
        let directory = repository.join(".oracle-lab/parity-corpus/cases/case-source");
        fs::create_dir_all(&directory).unwrap();
        write_test_source(&directory);
        write_manifest_idempotent(
            &directory.join("case.json"),
            &test_manifest("case-source", test_coverage()),
        )
        .unwrap();
        let error = replay_parity_corpus_v1(&repository).unwrap_err();
        assert!(error.contains("Java initial observation failed"), "{error}");
        fs::remove_dir_all(repository).unwrap();
    }

    /// A constructed recording envelope for source/receipt tests. Its empty
    /// frames intentionally cannot satisfy the current game projection.
    fn write_test_source(directory: &Path) -> RecordedJavaEvidenceV1 {
        use sts_parity_lockstep::{NaturalRunBootstrapV1, ProducerIdentityV1};
        use sts_parity_lockstep_java::{
            JavaLockstepReceiptV1, JavaRuntimeObservationV1, RecordedJavaActionPhaseV1,
            RecordedJavaEvidenceStoreV1,
        };
        let bootstrap: NaturalRunBootstrapV1 = sts_parity_lockstep_rust::natural_run_bootstrap_v1(
            &sts_oracle_run_control::eval::run_control::RunControlConfig {
                seed: 7,
                ascension_level: 20,
                final_act: true,
                neow_eligibility:
                    sts_oracle_run_control::content::events::neow::NeowEligibilityV1::Rich,
                ..Default::default()
            },
        )
        .unwrap();
        let mut store =
            RecordedJavaEvidenceStoreV1::create_new(&directory.join("java-evidence"), bootstrap)
                .unwrap();
        let before = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "java".into(),
                version: "v1".into(),
                session_epoch: "constructed".into(),
            },
            frame_id: 1,
            frame: serde_json::json!({}),
            actions: Vec::new(),
        };
        let mut after = before.clone();
        after.frame_id = 2;
        store.append_boundary(&before).unwrap();
        store
            .append_action(
                RecordedJavaActionPhaseV1::Committed,
                &test_coverage().transitions[0].action,
                &JavaLockstepReceiptV1 {
                    status: "completed".into(),
                    request_id: "request".into(),
                    based_on_frame_id: 1,
                    action_id: "proceed".into(),
                    successor_frame_id: Some(2),
                },
            )
            .unwrap();
        store.append_boundary(&after).unwrap();
        RecordedJavaEvidenceV1::load_complete(store.directory()).unwrap()
    }

    #[test]
    fn corpus_replay_rejects_an_empty_store() {
        let repository = temporary_repository("empty-replay");
        fs::create_dir_all(
            repository
                .join(".oracle-lab")
                .join("parity-corpus")
                .join("cases"),
        )
        .unwrap();

        assert_eq!(
            replay_parity_corpus_v1(&repository),
            Err("parity corpus has no retained cases".to_owned())
        );
        fs::remove_dir_all(repository).unwrap();
    }

    #[test]
    fn corpus_replay_rejects_a_renamed_case_before_loading_evidence() {
        let repository = temporary_repository("renamed-replay");
        let directory = repository
            .join(".oracle-lab")
            .join("parity-corpus")
            .join("cases")
            .join("renamed");
        fs::create_dir_all(&directory).unwrap();
        write_manifest_idempotent(
            &directory.join("case.json"),
            &test_manifest("original", test_coverage()),
        )
        .unwrap();

        assert_eq!(
            replay_parity_corpus_v1(&repository),
            Err("parity corpus case 0 directory identity does not match its manifest".to_owned())
        );
        fs::remove_dir_all(repository).unwrap();
    }

    fn test_manifest(
        case_id: &str,
        coverage: ParityCorpusCoverageV1,
    ) -> ParityCorpusCaseManifestV1 {
        ParityCorpusCaseManifestV1 {
            schema: PARITY_CORPUS_CASE_SCHEMA_V1.to_owned(),
            case_id: case_id.to_owned(),
            kind: ParityCorpusCaseKindV1::RunRngProbe,
            producer_implementation: "java".to_owned(),
            producer_version: "v1".to_owned(),
            coverage,
            committed_actions: 1,
            java_actions: 1,
        }
    }

    fn test_coverage() -> ParityCorpusCoverageV1 {
        let boundary = ParityCorpusBoundaryCoverageV1 {
            boundary_kind: "natural_neow".to_owned(),
            domains: vec![ParityCorpusDomainCoverageV1 {
                domain: ParityDomainV1::Run,
                observed_schema: Some("run-v1".to_owned()),
            }],
        };
        ParityCorpusCoverageV1 {
            player_class: "IRONCLAD".to_owned(),
            ascension: 20,
            final_act: true,
            content_profile: NaturalRunContentProfileV1::AllUnlocked,
            neow_eligibility: NaturalRunNeowEligibilityV1::Rich,
            initial: boundary.clone(),
            transitions: vec![ParityCorpusTransitionCoverageV1 {
                before: boundary.clone(),
                action: SemanticActionV1::Proceed {
                    surface: sts_parity_lockstep::DecisionSurfaceV1::Neow,
                },
                after: boundary,
            }],
        }
    }

    fn staging_case(repository: &Path, case_id: &str) -> PathBuf {
        let path = repository
            .join(".oracle-lab")
            .join("parity-corpus")
            .join("staging")
            .join(case_id);
        fs::create_dir_all(&path).unwrap();
        path
    }

    pub(super) fn temporary_repository(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sts-parity-corpus-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
}
