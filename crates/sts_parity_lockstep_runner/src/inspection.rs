//! Read-only inspection of an explicit recorded transition. All execution is
//! delegated to the normal offline verifier; reports are never replay inputs.
use crate::{manual_replay::replay_manual_scope_diagnosed, LockstepProgressV1, ManualSessionV1};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Instant};
use sts_parity_lockstep::{
    compare_natural_run_rng_calls_v1, compare_snapshots_v1, NaturalRunRngCallComparisonV1,
    NaturalRunRngCallLedgerV1, ParitySnapshotV1, SemanticActionV1, SnapshotComparisonV1,
};
use sts_parity_lockstep_java::{
    RecordedJavaActionV1, RecordedJavaBoundaryV1, RecordedJavaEvidenceV1,
};

#[derive(Clone, Debug, Serialize)]
pub struct ManualTransitionCoordinateV1 {
    pub session_id: String,
    /// One-based committed semantic action within this source session.
    pub action_number: usize,
}

#[derive(Debug, Serialize)]
pub struct ReplayScopeInspectionV1 {
    pub session_id: String,
    pub recorded_actions: Option<usize>,
    pub exact_actions: usize,
    pub load_ms: Option<u128>,
    pub total_ms: u128,
    pub stopped: bool,
}

#[derive(Debug, Serialize)]
pub struct InspectedRawJavaTransitionV1 {
    pub before: RecordedJavaBoundaryV1,
    pub action: RecordedJavaActionV1,
    pub after: RecordedJavaBoundaryV1,
}

#[derive(Debug, Serialize)]
pub struct InspectedTransitionV1 {
    pub coordinate: ManualTransitionCoordinateV1,
    pub action: SemanticActionV1,
    pub raw_java: Vec<InspectedRawJavaTransitionV1>,
    pub java_before: ParitySnapshotV1,
    pub rust_before: ParitySnapshotV1,
    pub java_after: Option<ParitySnapshotV1>,
    pub rust_after: Option<ParitySnapshotV1>,
    pub java_rng_calls: Option<NaturalRunRngCallLedgerV1>,
    pub rust_rng_calls: Option<NaturalRunRngCallLedgerV1>,
    pub snapshot_comparison: Option<SnapshotComparisonV1>,
    pub rng_comparison: Option<NaturalRunRngCallComparisonV1>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ManualTransitionInspectionV1 {
    pub schema: String,
    pub requested: ManualTransitionCoordinateV1,
    pub rust_package_version: String,
    pub reached_exact: bool,
    pub stop_reason: Option<String>,
    pub scopes: Vec<ReplayScopeInspectionV1>,
    /// Requested transition, or the first earlier transition that stopped replay.
    pub transition: Option<InspectedTransitionV1>,
    pub elapsed_ms: u128,
}

impl ManualTransitionInspectionV1 {
    pub(crate) fn finish_scope(
        &mut self,
        session_id: &str,
        recorded_actions: Option<usize>,
        exact_actions: usize,
        load_ms: Option<u128>,
        total_ms: u128,
        capture: TransitionCaptureV1,
        failure: Option<&String>,
    ) {
        self.scopes.push(ReplayScopeInspectionV1 {
            session_id: session_id.into(),
            recorded_actions,
            exact_actions,
            load_ms,
            total_ms,
            stopped: failure.is_some(),
        });
        if capture.transition.is_some() {
            self.transition = capture.transition;
        }
    }

    /// Default output contains only decision-sized facts. Full state is opt-in.
    pub fn summary(&self) -> String {
        let exact: usize = self.scopes.iter().map(|scope| scope.exact_actions).sum();
        let mut text = format!(
            "offline inspection {} | {} source scopes | {exact} verified actions | {:.2}s",
            if self.reached_exact {
                "Exact"
            } else {
                "stopped"
            },
            self.scopes.len(),
            self.elapsed_ms as f64 / 1000.0
        );
        if let Some(transition) = &self.transition {
            let scope = self
                .scopes
                .iter()
                .position(|scope| scope.session_id == transition.coordinate.session_id)
                .map(|index| index + 1)
                .unwrap_or(0);
            let identity = match &transition.action {
                SemanticActionV1::PlayCard { card_id, .. } => format!("play {card_id}"),
                SemanticActionV1::UsePotion { potion_id, .. } => format!("use {potion_id}"),
                action => action.kind_name().into(),
            };
            text.push_str(&format!(
                "\nsource {scope} action {} | {identity} | RNG calls Java {} / Rust {}",
                transition.coordinate.action_number,
                transition
                    .java_rng_calls
                    .as_ref()
                    .map(|v| v.0.len().to_string())
                    .unwrap_or("unavailable".into()),
                transition
                    .rust_rng_calls
                    .as_ref()
                    .map(|v| v.0.len().to_string())
                    .unwrap_or("unavailable".into())
            ));
            if transition.coordinate.session_id != self.requested.session_id
                || transition.coordinate.action_number != self.requested.action_number
            {
                text.push_str("\nrequested transition not reached; stopped at an earlier action");
            }
        }
        if let Some(reason) = &self.stop_reason {
            let short = reason.chars().take(320).collect::<String>();
            text.push_str(&format!("\n{short}"));
            if short.len() < reason.len() {
                text.push_str("… (full reason in inspection evidence)");
            }
        }
        text
    }
}

/// Reads explicitly selected evidence, including historical failed runs. The
/// active selector and Java lifecycle are neither consulted nor changed.
pub fn inspect_manual_transition_v1(
    session: &ManualSessionV1,
    action_number: usize,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<ManualTransitionInspectionV1, String> {
    if action_number == 0 {
        return Err("inspection action must be one-based and positive".into());
    }
    let started = Instant::now();
    let mut report = ManualTransitionInspectionV1 {
        schema: "sts-manual-transition-inspection-v1".into(),
        requested: ManualTransitionCoordinateV1 {
            session_id: session.session_id().into(),
            action_number,
        },
        rust_package_version: env!("CARGO_PKG_VERSION").into(),
        reached_exact: false,
        stop_reason: None,
        scopes: vec![],
        transition: None,
        elapsed_ms: 0,
    };
    let result = replay_manual_scope_diagnosed(
        session.repository(),
        session,
        Some(action_number),
        None,
        observer,
        Some(&mut report),
        None,
    );
    match result {
        Ok(_)
            if report.transition.as_ref().is_some_and(|t| {
                t.coordinate.session_id == session.session_id()
                    && t.coordinate.action_number == action_number
                    && t.stop_reason.is_none()
            }) =>
        {
            report.reached_exact = true
        }
        Ok(_) => report.stop_reason = Some("requested transition was not observed".into()),
        Err(reason) => report.stop_reason = Some(reason),
    }
    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

/// Saves diagnostic evidence with producer identity. There is deliberately no
/// load-as-state API. Existing reports and original source files are never replaced.
pub fn save_manual_transition_inspection_v1(
    repository: &Path,
    output: &Path,
    report: &ManualTransitionInspectionV1,
) -> Result<(), String> {
    let version = std::process::Command::new("git")
        .args(["describe", "--always", "--dirty", "--abbrev=40"])
        .current_dir(repository)
        .output()
        .map_err(|e| e.to_string())?;
    if !version.status.success() {
        return Err("cannot identify inspection producer source".into());
    }
    let executable =
        fs::read(std::env::current_exe().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let artifact = serde_json::json!({ "schema": "sts-manual-transition-inspection-artifact-v1",
        "producer": { "source_revision": String::from_utf8(version.stdout).map_err(|e| e.to_string())?.trim(),
            "executable_sha256": format!("{:x}", Sha256::digest(&executable)), "package_version": env!("CARGO_PKG_VERSION") },
        "inspection": report });
    crate::manual::write_json_create_new(output, &artifact)
}

pub(crate) struct TransitionCaptureV1 {
    session_id: String,
    target: Option<usize>,
    transition: Option<InspectedTransitionV1>,
}

impl TransitionCaptureV1 {
    pub(crate) fn new(session_id: &str, requested: Option<&ManualTransitionCoordinateV1>) -> Self {
        Self {
            session_id: session_id.into(),
            target: requested
                .filter(|r| r.session_id == session_id)
                .map(|r| r.action_number),
            transition: None,
        }
    }

    pub(crate) fn record(
        &mut self,
        action_number: usize,
        evidence: &RecordedJavaEvidenceV1,
        java_before: &ParitySnapshotV1,
        rust_before: &ParitySnapshotV1,
        java_after: Option<&ParitySnapshotV1>,
        rust_after: Option<&ParitySnapshotV1>,
        java_rng_calls: Option<&NaturalRunRngCallLedgerV1>,
        rust_rng_calls: Option<&NaturalRunRngCallLedgerV1>,
        failure: Option<&String>,
    ) {
        if self.target != Some(action_number) && failure.is_none() {
            return;
        }
        let step = &evidence.steps[action_number - 1];
        self.transition = Some(InspectedTransitionV1 {
            coordinate: ManualTransitionCoordinateV1 {
                session_id: self.session_id.clone(),
                action_number,
            },
            action: step.action.clone(),
            raw_java: evidence
                .transitions
                .iter()
                .filter(|raw| step.java_actions.contains(&raw.action))
                .map(|raw| InspectedRawJavaTransitionV1 {
                    before: raw.before.clone(),
                    action: raw.action.clone(),
                    after: raw.after.clone(),
                })
                .collect(),
            java_before: java_before.clone(),
            rust_before: rust_before.clone(),
            java_after: java_after.cloned(),
            rust_after: rust_after.cloned(),
            java_rng_calls: java_rng_calls.cloned(),
            rust_rng_calls: rust_rng_calls.cloned(),
            snapshot_comparison: java_after
                .zip(rust_after)
                .and_then(|(j, r)| compare_snapshots_v1(j, r).ok()),
            rng_comparison: java_rng_calls
                .zip(rust_rng_calls)
                .and_then(|(j, r)| compare_natural_run_rng_calls_v1(j, r).ok()),
            stop_reason: failure.cloned(),
        });
    }
}

#[cfg(test)]
#[path = "inspection_tests.rs"]
mod tests;
