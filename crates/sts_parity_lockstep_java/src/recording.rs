use crate::{
    projection::reproject_recorded_boundary_v1, JavaLockstepReceiptV1, JavaNativeReloadExecutionV1,
    JavaRuntimeObservationV1,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use sts_parity_lockstep::{NaturalRunBootstrapV1, ParitySnapshotV1, SemanticActionV1};

pub const RECORDED_JAVA_BOUNDARY_SCHEMA_V1: &str = "sts-recorded-java-boundary-v1";
pub const RECORDED_JAVA_ACTION_SCHEMA_V1: &str = "sts-recorded-java-action-v1";
pub const RECORDED_JAVA_EVIDENCE_MANIFEST_SCHEMA_V1: &str =
    "sts-recorded-java-evidence-manifest-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordedJavaActionPhaseV1 {
    /// Closed preparation after native Continue, before the first comparable
    /// boundary. Never a player choice or a committed trajectory action.
    NativeReloadSettlement,
    Committed,
    /// A closed Java-only UI action performed while settling the preceding
    /// committed action. Its semantic action records what Java actually
    /// executed and therefore need not equal the committed action.
    InterstitialSettlement,
}

/// Raw, stable Java evidence retained independently from any particular
/// canonical projection implementation.
///
/// This type deliberately contains the CommunicationMod frame and its action
/// surface, not a `ParitySnapshotV1`. A newer Rust projection can therefore
/// reinterpret an old observation without launching Java again, provided the
/// producer and protocol contract remain compatible.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedJavaBoundaryV1 {
    pub schema: String,
    pub observation: JavaRuntimeObservationV1,
}

impl RecordedJavaBoundaryV1 {
    pub fn new(observation: JavaRuntimeObservationV1) -> Result<Self, String> {
        let recorded = Self {
            schema: RECORDED_JAVA_BOUNDARY_SCHEMA_V1.to_owned(),
            observation,
        };
        recorded.validate()?;
        Ok(recorded)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != RECORDED_JAVA_BOUNDARY_SCHEMA_V1 {
            return Err(format!(
                "unsupported recorded Java boundary schema {:?}",
                self.schema
            ));
        }
        if self.observation.frame_id == 0 {
            return Err("recorded Java boundary has frame ID zero".to_owned());
        }
        for (label, value) in [
            (
                "producer implementation",
                self.observation.producer.implementation.as_str(),
            ),
            (
                "producer version",
                self.observation.producer.version.as_str(),
            ),
            (
                "producer session epoch",
                self.observation.producer.session_epoch.as_str(),
            ),
        ] {
            if value.is_empty() {
                return Err(format!("recorded Java boundary has empty {label}"));
            }
        }
        if !self.observation.frame.is_object() {
            return Err("recorded Java boundary frame is not an object".to_owned());
        }
        Ok(())
    }

    pub fn reproject(&self, bootstrap: &NaturalRunBootstrapV1) -> Result<ParitySnapshotV1, String> {
        self.validate()?;
        reproject_recorded_boundary_v1(&self.observation, bootstrap)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedJavaActionV1 {
    pub schema: String,
    pub phase: RecordedJavaActionPhaseV1,
    pub action: SemanticActionV1,
    pub receipt: JavaLockstepReceiptV1,
}

impl RecordedJavaActionV1 {
    pub fn new(
        phase: RecordedJavaActionPhaseV1,
        action: SemanticActionV1,
        receipt: JavaLockstepReceiptV1,
    ) -> Result<Self, String> {
        let recorded = Self {
            schema: RECORDED_JAVA_ACTION_SCHEMA_V1.to_owned(),
            phase,
            action,
            receipt,
        };
        recorded.validate()?;
        Ok(recorded)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != RECORDED_JAVA_ACTION_SCHEMA_V1 {
            return Err(format!(
                "unsupported recorded Java action schema {:?}",
                self.schema
            ));
        }
        self.action.validate()?;
        if self.receipt.status.is_empty()
            || self.receipt.request_id.is_empty()
            || self.receipt.action_id.is_empty()
        {
            return Err("recorded Java action has an empty receipt identity".to_owned());
        }
        if self.receipt.based_on_frame_id == 0 {
            return Err("recorded Java action has predecessor frame ID zero".to_owned());
        }
        if self.receipt.successor_frame_id == Some(self.receipt.based_on_frame_id) {
            return Err("recorded Java action reuses its predecessor frame ID".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedJavaEvidenceManifestV1 {
    pub schema: String,
    pub bootstrap: NaturalRunBootstrapV1,
}

impl RecordedJavaEvidenceManifestV1 {
    pub fn new(bootstrap: NaturalRunBootstrapV1) -> Result<Self, String> {
        let manifest = Self {
            schema: RECORDED_JAVA_EVIDENCE_MANIFEST_SCHEMA_V1.to_owned(),
            bootstrap,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != RECORDED_JAVA_EVIDENCE_MANIFEST_SCHEMA_V1 {
            return Err(format!(
                "unsupported recorded Java evidence manifest schema {:?}",
                self.schema
            ));
        }
        self.bootstrap.validate()
    }
}

#[derive(Debug)]
pub struct RecordedJavaEvidenceStoreV1 {
    directory: PathBuf,
    manifest: RecordedJavaEvidenceManifestV1,
    next_boundary_ordinal: u64,
    next_action_ordinal: u64,
    resume_tail: Option<RecordedJavaBoundaryV1>,
    committed_actions: usize,
    tail: Option<RecordedJavaBoundaryV1>,
    committed_before: Option<RecordedJavaBoundaryV1>,
    pending_receipt: Option<JavaLockstepReceiptV1>,
    continuous: bool,
}

/// A borrowed view of the writer's last complete semantic step. It cannot be
/// deserialized or carried across writes/reopens. This proves raw persistence,
/// not parity: consumers must separately require the runner's journal commit.
pub struct RecordedJavaStepRefV1<'a> {
    store: &'a RecordedJavaEvidenceStoreV1,
    before: &'a JavaRuntimeObservationV1,
    after: &'a JavaRuntimeObservationV1,
}

impl RecordedJavaStepRefV1<'_> {
    pub fn directory(&self) -> &Path {
        self.store.directory()
    }

    pub fn committed_actions(&self) -> usize {
        self.store.committed_actions
    }

    pub fn before(&self) -> &JavaRuntimeObservationV1 {
        self.before
    }

    pub fn after(&self) -> &JavaRuntimeObservationV1 {
        self.after
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordedJavaCommittedStepV1 {
    pub action: SemanticActionV1,
    pub before: RecordedJavaBoundaryV1,
    pub after: RecordedJavaBoundaryV1,
    pub java_actions: Vec<RecordedJavaActionV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordedJavaTransitionV1 {
    pub action: RecordedJavaActionV1,
    pub before: RecordedJavaBoundaryV1,
    pub after: RecordedJavaBoundaryV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordedJavaEvidenceV1 {
    pub manifest: RecordedJavaEvidenceManifestV1,
    pub initial: RecordedJavaBoundaryV1,
    pub native_reload: Option<JavaNativeReloadExecutionV1>,
    pub steps: Vec<RecordedJavaCommittedStepV1>,
    /// Every actual Java action, including closed interstitial settlement,
    /// paired with its adjacent raw boundaries. Offline transports replay
    /// these transitions through the same stateful Java adapter as live runs.
    pub transitions: Vec<RecordedJavaTransitionV1>,
    pub java_action_count: usize,
}

impl RecordedJavaEvidenceV1 {
    /// Loads only a complete successful prefix. Incomplete or rejected raw
    /// evidence remains readable file-by-file, but cannot masquerade as a
    /// replayable trajectory.
    pub fn load_complete(directory: &Path) -> Result<Self, String> {
        let _timing = sts_parity_lockstep::timing_span_v1("evidence.load");
        Self::load_scope(directory, None)
    }

    /// Read a successful prefix including its closed Java settlements. The next
    /// action's phase delimits the prefix; later payloads are not interpreted.
    /// An unreadable delimiter or a hole within the prefix remains an error.
    pub fn load_prefix(directory: &Path, committed_actions: usize) -> Result<Self, String> {
        let _timing = sts_parity_lockstep::timing_span_v1("evidence.load_prefix");
        Self::load_scope(directory, Some(committed_actions))
    }

    fn load_scope(directory: &Path, committed_actions: Option<usize>) -> Result<Self, String> {
        let manifest =
            RecordedJavaEvidenceStoreV1::load_manifest(&directory.join("manifest.json"))?;
        let mut actions = Vec::new();
        let mut seen_committed = 0usize;
        let action_paths = ordered_record_paths(&directory.join("actions"), "action")?;
        for (index, (ordinal, path)) in action_paths.iter().enumerate() {
            require_record_ordinal(
                *ordinal,
                index,
                action_paths.get(index + 1).map(|p| p.0),
                "action",
            )?;
            let bytes = fs::read(path)
                .map_err(|error| format!("failed to read recorded Java action: {error}"))?;
            if committed_actions == Some(seen_committed) {
                // We still need to distinguish a settlement of the final chosen
                // action from the first unchosen action. Its receipt and action
                // body cannot authorize this prefix and need not be decoded.
                #[derive(Deserialize)]
                struct ActionPhase {
                    schema: String,
                    phase: RecordedJavaActionPhaseV1,
                }
                let delimiter: ActionPhase = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("cannot delimit recorded Java prefix: {error}"))?;
                if delimiter.schema != RECORDED_JAVA_ACTION_SCHEMA_V1 {
                    return Err("unsupported recorded Java prefix delimiter schema".to_owned());
                }
                if delimiter.phase == RecordedJavaActionPhaseV1::Committed {
                    break;
                }
            }
            let action = decode_recorded_action(&bytes)?;
            if action.phase == RecordedJavaActionPhaseV1::Committed {
                seen_committed += 1;
            }
            actions.push(action);
        }
        if committed_actions.is_some_and(|requested| requested > seen_committed) {
            return Err(format!(
                "requested {} committed actions, but evidence contains only {seen_committed}",
                committed_actions.unwrap()
            ));
        }
        let paths = ordered_record_paths(&directory.join("boundaries"), "boundary")?;
        let required = if committed_actions.is_some() {
            actions.len() + 1
        } else {
            paths.len()
        };
        let boundaries = paths
            .iter()
            .take(required)
            .enumerate()
            .map(|(index, (ordinal, path))| {
                require_record_ordinal(
                    *ordinal,
                    index,
                    paths.get(index + 1).map(|p| p.0),
                    "boundary",
                )?;
                RecordedJavaEvidenceStoreV1::load_boundary(path)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let initial = boundaries
            .first()
            .cloned()
            .ok_or_else(|| "recorded Java evidence has no initial boundary".to_owned())?;
        let reload_path = directory.join("native-reload.json");
        let native_reload = match fs::read(&reload_path) {
            Ok(bytes) => {
                let execution: JavaNativeReloadExecutionV1 = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("invalid native reload execution: {error}"))?;
                execution.validate()?;
                if execution.after != initial.observation {
                    return Err(
                        "native reload execution differs from the initial recorded boundary"
                            .to_owned(),
                    );
                }
                Some(execution)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("failed to read native reload execution: {error}")),
        };
        if boundaries.len() != actions.len() + 1 {
            return Err(format!(
                "recorded Java evidence has {} boundaries for {} Java actions",
                boundaries.len(),
                actions.len()
            ));
        }
        for (index, action) in actions.iter().enumerate() {
            let before = &boundaries[index];
            let after = &boundaries[index + 1];
            if !receipt_links_boundaries(&action.receipt, &before.observation, &after.observation) {
                return Err(format!(
                    "recorded Java action {index} does not link its adjacent boundaries"
                ));
            }
            if after.observation.producer != initial.observation.producer {
                return Err(format!(
                    "recorded Java boundary {} changed producer identity",
                    index + 1
                ));
            }
        }

        let transitions = actions
            .iter()
            .cloned()
            .zip(boundaries.windows(2))
            .map(|(action, pair)| RecordedJavaTransitionV1 {
                action,
                before: pair[0].clone(),
                after: pair[1].clone(),
            })
            .collect::<Vec<_>>();

        let mut steps = Vec::new();
        let mut index = actions
            .iter()
            .take_while(|action| action.phase == RecordedJavaActionPhaseV1::NativeReloadSettlement)
            .count();
        while index < actions.len() {
            let committed = &actions[index];
            if committed.phase != RecordedJavaActionPhaseV1::Committed {
                return Err(format!(
                    "recorded Java action {index} is settlement without a committed action"
                ));
            }
            let action = committed.action.clone();
            let start = index;
            index += 1;
            while index < actions.len()
                && actions[index].phase == RecordedJavaActionPhaseV1::InterstitialSettlement
            {
                index += 1;
            }
            steps.push(RecordedJavaCommittedStepV1 {
                action,
                before: boundaries[start].clone(),
                after: boundaries[index].clone(),
                java_actions: actions[start..index].to_vec(),
            });
        }

        Ok(Self {
            manifest,
            initial,
            native_reload,
            steps,
            transitions,
            java_action_count: actions.len(),
        })
    }
}

impl RecordedJavaEvidenceStoreV1 {
    pub fn record_native_reload(
        &self,
        execution: &JavaNativeReloadExecutionV1,
    ) -> Result<(), String> {
        execution.validate()?;
        if self.next_boundary_ordinal != 0 || self.next_action_ordinal != 0 {
            return Err(
                "native reload execution must precede the first recorded boundary".to_owned(),
            );
        }
        write_json_create_new(&self.directory.join("native-reload.json"), execution)
    }

    pub fn create_new(directory: &Path, bootstrap: NaturalRunBootstrapV1) -> Result<Self, String> {
        let manifest = RecordedJavaEvidenceManifestV1::new(bootstrap)?;
        let parent = directory
            .parent()
            .ok_or_else(|| "recorded Java boundary store has no parent".to_owned())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create Java boundary store parent: {error}"))?;
        fs::create_dir(directory)
            .map_err(|error| format!("failed to create Java evidence store: {error}"))?;
        fs::create_dir(directory.join("boundaries"))
            .map_err(|error| format!("failed to create Java boundary directory: {error}"))?;
        fs::create_dir(directory.join("actions"))
            .map_err(|error| format!("failed to create Java action directory: {error}"))?;
        write_json_create_new(&directory.join("manifest.json"), &manifest)?;
        Ok(Self {
            directory: directory.to_path_buf(),
            manifest,
            next_boundary_ordinal: 0,
            next_action_ordinal: 0,
            resume_tail: None,
            committed_actions: 0,
            tail: None,
            committed_before: None,
            pending_receipt: None,
            continuous: true,
        })
    }

    /// Reopens one complete raw trajectory for append-only continuation.
    ///
    /// The first boundary observed by the resumed live adapter must be exactly
    /// the already-recorded tail. That observation is acknowledged without
    /// writing a duplicate boundary; every later action and successor keeps
    /// the original ordinal sequence.
    pub fn open_append(directory: &Path) -> Result<Self, String> {
        let evidence = RecordedJavaEvidenceV1::load_complete(directory)?;
        let next_action_ordinal = u64::try_from(evidence.java_action_count)
            .map_err(|_| "recorded Java action count does not fit u64".to_owned())?;
        let next_boundary_ordinal = next_action_ordinal
            .checked_add(1)
            .ok_or_else(|| "recorded Java boundary ordinal overflowed u64".to_owned())?;
        let resume_tail = evidence
            .transitions
            .last()
            .map(|transition| transition.after.clone())
            .unwrap_or_else(|| evidence.initial.clone());
        Ok(Self {
            directory: directory.to_path_buf(),
            manifest: evidence.manifest,
            next_boundary_ordinal,
            next_action_ordinal,
            committed_actions: evidence.steps.len(),
            tail: Some(resume_tail.clone()),
            resume_tail: Some(resume_tail),
            // A reopened tail is not a newly executed step.
            committed_before: None,
            pending_receipt: None,
            continuous: true,
        })
    }

    pub fn append_boundary(
        &mut self,
        observation: &JavaRuntimeObservationV1,
    ) -> Result<PathBuf, String> {
        let _timing = sts_parity_lockstep::timing_span_v1("evidence.write_boundary");
        let recorded = RecordedJavaBoundaryV1::new(observation.clone())?;
        if let Some(expected) = self.resume_tail.take() {
            if recorded != expected {
                self.continuous = false;
                return Err(
                    "resumed live Java boundary differs from the recorded evidence tail".to_owned(),
                );
            }
            let ordinal = self
                .next_boundary_ordinal
                .checked_sub(1)
                .ok_or_else(|| "recorded Java evidence has no resume tail".to_owned())?;
            return Ok(self
                .directory
                .join("boundaries")
                .join(format!("{ordinal:08}-frame-{}.json", observation.frame_id)));
        }
        let path = self.directory.join("boundaries").join(format!(
            "{:08}-frame-{}.json",
            self.next_boundary_ordinal, observation.frame_id
        ));
        let was_continuous = self.continuous;
        self.continuous = false;
        write_json_create_new(&path, &recorded)?;
        self.continuous = was_continuous;
        if let Some(before) = self.tail.as_ref() {
            self.continuous &= self.pending_receipt.take().is_some_and(|receipt| {
                receipt_links_boundaries(&receipt, &before.observation, observation)
                    && observation.producer == before.observation.producer
            });
        } else {
            self.continuous &= self.pending_receipt.is_none();
        }
        self.tail = Some(recorded);
        self.next_boundary_ordinal = self
            .next_boundary_ordinal
            .checked_add(1)
            .ok_or_else(|| "recorded Java boundary ordinal overflowed u64".to_owned())?;
        Ok(path)
    }

    pub fn append_action(
        &mut self,
        phase: RecordedJavaActionPhaseV1,
        action: &SemanticActionV1,
        receipt: &JavaLockstepReceiptV1,
    ) -> Result<PathBuf, String> {
        let _timing = sts_parity_lockstep::timing_span_v1("evidence.write_receipt");
        if self.resume_tail.is_some() {
            return Err(
                "resumed Java evidence must confirm its recorded tail before appending an action"
                    .to_owned(),
            );
        }
        let recorded = RecordedJavaActionV1::new(phase, action.clone(), receipt.clone())?;
        let path = self.directory.join("actions").join(format!(
            "{:08}-from-frame-{}.json",
            self.next_action_ordinal, receipt.based_on_frame_id
        ));
        let was_continuous = self.continuous;
        self.continuous = false;
        write_json_create_new(&path, &recorded)?;
        self.continuous = was_continuous;
        self.continuous &= self.pending_receipt.is_none()
            && self.tail.as_ref().is_some_and(|tail| {
                tail.observation.frame_id == receipt.based_on_frame_id
                    && receipt.status == "completed"
            });
        match phase {
            RecordedJavaActionPhaseV1::Committed => {
                self.committed_actions = self
                    .committed_actions
                    .checked_add(1)
                    .ok_or("recorded Java committed action count overflowed")?;
                self.committed_before = self.tail.clone();
            }
            RecordedJavaActionPhaseV1::InterstitialSettlement => {
                self.continuous &= self.committed_before.is_some();
            }
            RecordedJavaActionPhaseV1::NativeReloadSettlement => {
                self.continuous &= self.committed_actions == 0;
            }
        }
        self.pending_receipt = Some(receipt.clone());
        self.next_action_ordinal = self
            .next_action_ordinal
            .checked_add(1)
            .ok_or_else(|| "recorded Java action ordinal overflowed u64".to_owned())?;
        Ok(path)
    }

    pub fn load_boundary(path: &Path) -> Result<RecordedJavaBoundaryV1, String> {
        let bytes = fs::read(path)
            .map_err(|error| format!("failed to read recorded Java boundary: {error}"))?;
        let recorded: RecordedJavaBoundaryV1 = serde_json::from_slice(&bytes)
            .map_err(|error| format!("failed to decode recorded Java boundary: {error}"))?;
        recorded.validate()?;
        Ok(recorded)
    }

    pub fn load_action(path: &Path) -> Result<RecordedJavaActionV1, String> {
        let bytes = fs::read(path)
            .map_err(|error| format!("failed to read recorded Java action: {error}"))?;
        decode_recorded_action(&bytes)
    }

    pub fn load_manifest(path: &Path) -> Result<RecordedJavaEvidenceManifestV1, String> {
        let bytes = fs::read(path)
            .map_err(|error| format!("failed to read recorded Java evidence manifest: {error}"))?;
        let manifest: RecordedJavaEvidenceManifestV1 =
            serde_json::from_slice(&bytes).map_err(|error| {
                format!("failed to decode recorded Java evidence manifest: {error}")
            })?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn last_complete_step_v1(&self) -> Option<RecordedJavaStepRefV1<'_>> {
        if !self.continuous || self.resume_tail.is_some() || self.pending_receipt.is_some() {
            return None;
        }
        Some(RecordedJavaStepRefV1 {
            store: self,
            before: &self.committed_before.as_ref()?.observation,
            after: &self.tail.as_ref()?.observation,
        })
    }

    pub fn bootstrap(&self) -> &NaturalRunBootstrapV1 {
        &self.manifest.bootstrap
    }

    /// Raw epoch/frame equality used only to choose reattachment or native
    /// recovery. This does not validate Rust semantics or authorize any action;
    /// either path must still reconstruct bindings by current-code replay.
    pub fn matches_resume_observation_v1(&self, observation: &JavaRuntimeObservationV1) -> bool {
        self.resume_tail
            .as_ref()
            .is_some_and(|tail| &tail.observation == observation)
    }
}

fn receipt_links_boundaries(
    receipt: &JavaLockstepReceiptV1,
    before: &JavaRuntimeObservationV1,
    after: &JavaRuntimeObservationV1,
) -> bool {
    receipt.status == "completed"
        && receipt.based_on_frame_id == before.frame_id
        && receipt.successor_frame_id == Some(after.frame_id)
        && after.frame_id > before.frame_id
}

fn write_json_create_new<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode recorded Java boundary: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create recorded Java boundary: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist recorded Java boundary: {error}"))
}

fn decode_recorded_action(bytes: &[u8]) -> Result<RecordedJavaActionV1, String> {
    let recorded: RecordedJavaActionV1 = serde_json::from_slice(bytes)
        .map_err(|error| format!("failed to decode recorded Java action: {error}"))?;
    recorded.validate()?;
    Ok(recorded)
}

fn require_record_ordinal(
    ordinal: u64,
    index: usize,
    next: Option<u64>,
    label: &str,
) -> Result<(), String> {
    if usize::try_from(ordinal) != Ok(index) || next == Some(ordinal) {
        return Err(format!(
            "recorded Java {label} sequence has a gap or duplicate at {index}"
        ));
    }
    Ok(())
}

fn ordered_record_paths(directory: &Path, label: &str) -> Result<Vec<(u64, PathBuf)>, String> {
    let paths = fs::read_dir(directory)
        .map_err(|error| format!("failed to read recorded Java {label} directory: {error}"))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("failed to read recorded Java {label} entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if paths.iter().any(|path| {
        !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json")
    }) {
        return Err(format!(
            "recorded Java {label} directory contains an unexpected entry"
        ));
    }
    let mut indexed = paths
        .into_iter()
        .map(|path| {
            let ordinal = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.split_once('-'))
                .and_then(|(ordinal, _)| ordinal.parse::<u64>().ok())
                .ok_or_else(|| format!("recorded Java {label} filename has no ordinal"))?;
            Ok((ordinal, path))
        })
        .collect::<Result<Vec<_>, String>>()?;
    indexed.sort_by_key(|(ordinal, _)| *ordinal);
    Ok(indexed)
}
