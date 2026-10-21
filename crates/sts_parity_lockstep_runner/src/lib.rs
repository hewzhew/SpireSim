//! High-throughput parity lab operations shared by thin frontends.
//!
//! Offline replay is deliberately read-only: it reprojects recorded raw Java
//! evidence and rebuilds the Rust endpoint without discovering or launching a
//! JVM and without writing a second journal.

mod act;
mod corpus;
mod host;
mod inspection;
mod manual;
mod manual_replay;
pub mod mechanism;
mod native;
mod progress;
mod replay_suite;
mod training_trajectory;
pub use inspection::*;
pub use replay_suite::*;
pub use training_trajectory::*;

pub use act::*;
pub use corpus::*;
pub use host::*;
pub use manual::*;
pub use manual_replay::*;
pub use native::*;
pub use progress::LockstepProgressV1;
mod manual_draft;
pub use manual_draft::*;
mod draft_adoption;
pub use draft_adoption::*;
mod draft_workspace;
pub use draft_workspace::*;
mod decision_service;
pub use decision_service::*;
mod decision_map;
mod decision_match;
mod decision_presentation;
mod decision_sections;
mod decision_shop;
mod presentation_names;
pub use decision_presentation::{render_decision_choices_v1, DecisionSectionV1, DecisionStateV1};
mod reward_plan;
pub use reward_plan::*;

use std::path::Path;
use sts_oracle_run_control::eval::run_control::RunControlSession;
use sts_parity_lockstep::{
    compare_natural_run_rng_calls_v1, compare_snapshots_v1, AdapterFailureKindV1, AdapterFailureV1,
    ComparisonStatusV1, DecisionSurfaceV1, LockstepAdapterV1, LockstepModeV1, LockstepRunnerV1,
    NaturalRunBootstrapV1, ParityDomainV1, SemanticActionV1, SnapshotComparisonV1,
};
use sts_parity_lockstep_java::{
    JavaLockstepAdapterV1, JavaLockstepReceiptV1, JavaLockstepResumeStateV1,
    JavaLockstepTransportV1, JavaRuntimeObservationV1, RecordedJavaEvidenceStoreV1,
    RecordedJavaEvidenceV1, RecordedJavaTransitionV1,
};
use sts_parity_lockstep_rust::{run_control_config_from_bootstrap_v1, RustLockstepAdapterV1};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct OfflineReplayReportV1 {
    pub domains: usize,
    pub committed_actions: usize,
    pub java_actions: usize,
    pub coverage: ReplayCoverageV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunRngProbeReportV1 {
    pub domains: usize,
    pub committed_actions: usize,
    pub end_boundary: String,
}

pub fn is_neow_proceed_v1(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::Neow
        }
    )
}

pub fn is_immediate_neow_reward_v1(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Neow,
            choice,
        } if choice.kind == "neow_reward" && matches!(
            choice.id.as_str(),
            "one_random_rare_card"
                | "random_common_relic"
                | "ten_percent_hp_bonus"
                | "hundred_gold"
                | "three_enemy_kill"
                | "one_rare_relic"
                | "two_fifty_gold"
                | "twenty_percent_hp_bonus"
        )
    )
}

pub fn is_map_choice_v1(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            ..
        }
    )
}

/// Record one deliberately short live Run + RNG case after the host has
/// started a natural run.
///
/// The case owns the short natural prefix from Neow introduction through the
/// first combat entry, persists raw Java boundaries in managed corpus staging,
/// and returns both that candidate path and the leased transport to its caller.
/// Host lifecycle and between-case cleanup remain outside this function, so a
/// caller can run a batch in one resident JVM.
pub fn record_started_run_rng_probe_v1<T>(
    repository: &Path,
    java_transport: T,
    config: sts_oracle_run_control::eval::run_control::RunControlConfig,
    bootstrap: NaturalRunBootstrapV1,
) -> Result<(RunRngProbeReportV1, std::path::PathBuf, T), String>
where
    T: JavaLockstepTransportV1,
{
    let journal_id = format!("run-rng-probe-{}", Uuid::new_v4().simple());
    let journal = parity_corpus_staging_case_v1(repository, &journal_id)?;
    let evidence_store =
        RecordedJavaEvidenceStoreV1::create_new(&journal.join("java-evidence"), bootstrap.clone())?;
    let java = JavaLockstepAdapterV1::with_evidence_store(
        java_transport,
        bootstrap.clone(),
        evidence_store,
    )?;
    let rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap,
        env!("CARGO_PKG_VERSION"),
        Uuid::new_v4().simple().to_string(),
    )?;
    let mut runner =
        LockstepRunnerV1::start_new(java, rust, LockstepModeV1::Audit, &journal, &journal_id)?;
    require_exact("initial Run + RNG boundary", runner.current_comparison())?;
    let initial_boundary = runner.current_snapshots().0.boundary_kind.as_str();
    if initial_boundary != "natural_neow" {
        return Err(format!(
            "Run + RNG probe expected natural Neow, observed {initial_boundary}"
        ));
    }

    step_probe_action(&mut runner, is_neow_proceed_v1, true, "Neow introduction")?;
    step_probe_action(
        &mut runner,
        is_immediate_neow_reward_v1,
        false,
        "immediate Neow reward",
    )?;
    step_probe_action(&mut runner, is_neow_proceed_v1, true, "Neow leave")?;
    step_probe_action(&mut runner, is_map_choice_v1, false, "first map node")?;
    let comparison = runner.current_comparison();
    require_exact("Run + RNG prefix successor", comparison)?;
    let rng = comparison
        .domains
        .iter()
        .find(|domain| domain.domain == ParityDomainV1::RngStreams)
        .ok_or_else(|| "Run + RNG probe comparison omitted the RNG domain".to_owned())?;
    if rng.status != ComparisonStatusV1::Exact {
        return Err(rng
            .reason
            .clone()
            .unwrap_or_else(|| "Run + RNG successor was not exact".to_owned()));
    }
    let end_boundary = runner.current_snapshots().0.boundary_kind.clone();
    if end_boundary != "natural_combat" {
        return Err(format!(
            "Run + RNG probe did not reach the first combat entry; observed {end_boundary}"
        ));
    }
    let domains = comparison.domains.len();
    let (java, _rust) = runner.into_adapters();
    Ok((
        RunRngProbeReportV1 {
            domains,
            committed_actions: 4,
            end_boundary,
        },
        journal,
        java.into_transport(),
    ))
}

fn step_probe_action<J, R>(
    runner: &mut LockstepRunnerV1<J, R>,
    predicate: fn(&SemanticActionV1) -> bool,
    require_unique: bool,
    label: &str,
) -> Result<(), String>
where
    J: LockstepAdapterV1,
    R: LockstepAdapterV1,
{
    let mut actions = runner
        .current_action_space()?
        .actions
        .into_iter()
        .filter(|candidate| predicate(&candidate.action))
        .collect::<Vec<_>>();
    actions.sort_by_key(|candidate| match &candidate.action {
        SemanticActionV1::Select { choice, .. } => choice.occurrence,
        _ => u32::MAX,
    });
    if actions.is_empty() || (require_unique && actions.len() != 1) {
        return Err(format!(
            "Run + RNG probe resolved {} {label} actions{}",
            actions.len(),
            if require_unique {
                " instead of one"
            } else {
                ""
            }
        ));
    }
    let entry = runner.step(actions.remove(0).action)?;
    if runner.is_stopped() {
        return Err(runner
            .stop_reason()
            .unwrap_or("Run + RNG probe stopped without a reason")
            .to_owned());
    }
    if !matches!(
        entry.outcome,
        sts_parity_lockstep::TransitionOutcomeV1::Continued { .. }
    ) {
        return Err(format!("Run + RNG probe {label} did not continue"));
    }
    Ok(())
}

pub fn replay_recorded_evidence_v1(directory: &Path) -> Result<OfflineReplayReportV1, String> {
    replay_recorded_evidence_into_rust_v1(directory).map(|(report, _rust)| report)
}

/// Visits the initial state and each exact committed successor during the
/// ordinary raw-evidence replay. Inputs belong to the transition just checked.
/// A visitor can validate another caller against this same trajectory without
/// launching Java, duplicating replay rules, or trusting stored projections.
pub fn replay_recorded_evidence_with_observer_v1(
    directory: &Path,
    observer: &mut dyn FnMut(
        usize,
        &RustLockstepAdapterV1,
        &sts_parity_lockstep::ExecutionInputsV1,
    ) -> Result<(), String>,
) -> Result<OfflineReplayReportV1, String> {
    let evidence = RecordedJavaEvidenceV1::load_complete(directory)?;
    replay_recorded_evidence_observed_v1(
        &evidence,
        evidence.steps.len(),
        true,
        None,
        &mut |index, rust, java| observer(index, rust, &java.last_execution_inputs_v1()),
    )
    .map(|(report, _, _)| report)
}

/// Reprojects every recorded Java boundary, replays every committed action in
/// Rust, and returns the exact terminal Rust adapter for further offline work.
/// This neither writes the evidence directory nor discovers or launches Java.
pub fn replay_recorded_evidence_into_rust_v1(
    directory: &Path,
) -> Result<(OfflineReplayReportV1, RustLockstepAdapterV1), String> {
    replay_recorded_evidence_for_resume_v1(directory)
        .map(|(report, rust, _java_resume)| (report, rust))
}

/// Replays a complete raw trajectory into the current Rust implementation and
/// reconstructs the Java adapter's runtime identity bindings. The returned
/// opaque Java state can attach a new worker to the exact parked live boundary
/// without replaying the trajectory in the JVM.
pub fn replay_recorded_evidence_for_resume_v1(
    directory: &Path,
) -> Result<
    (
        OfflineReplayReportV1,
        RustLockstepAdapterV1,
        JavaLockstepResumeStateV1,
    ),
    String,
> {
    let evidence = RecordedJavaEvidenceV1::load_complete(directory)?;
    replay_recorded_evidence_prefix_v1(&evidence, evidence.steps.len(), true, None)
}

/// Replays a trajectory whose first Java boundary is paired with an explicit
/// current-semantics Rust root rather than a new-run bootstrap boundary.
pub fn replay_recorded_evidence_from_rust_adapter_for_resume_v1(
    directory: &Path,
    rust: RustLockstepAdapterV1,
    java_reload: JavaLockstepResumeStateV1,
) -> Result<
    (
        OfflineReplayReportV1,
        RustLockstepAdapterV1,
        JavaLockstepResumeStateV1,
    ),
    String,
> {
    let evidence = RecordedJavaEvidenceV1::load_complete(directory)?;
    replay_recorded_evidence_prefix_v1(
        &evidence,
        evidence.steps.len(),
        true,
        Some((rust, java_reload)),
    )
}

/// Reprojects an explicitly bounded successful prefix and returns the Rust
/// adapter at that boundary. Later Java actions are not replayed; no JVM is discovered or
/// launched and the evidence directory is never modified.
pub fn replay_recorded_evidence_prefix_into_rust_v1(
    directory: &Path,
    committed_actions: usize,
) -> Result<(OfflineReplayReportV1, RustLockstepAdapterV1), String> {
    let evidence = RecordedJavaEvidenceV1::load_prefix(directory, committed_actions)?;
    replay_recorded_evidence_prefix_v1(&evidence, committed_actions, false, None)
        .map(|(report, rust, _java_resume)| (report, rust))
}

/// Replays an exact prefix from a natural new-run root and also reconstructs
/// the Java adapter bindings needed to reload that exact seeded boundary.
pub fn replay_recorded_evidence_prefix_for_resume_v1(
    directory: &Path,
    committed_actions: usize,
) -> Result<
    (
        OfflineReplayReportV1,
        RustLockstepAdapterV1,
        JavaLockstepResumeStateV1,
    ),
    String,
> {
    let evidence = RecordedJavaEvidenceV1::load_prefix(directory, committed_actions)?;
    replay_recorded_evidence_prefix_v1(&evidence, committed_actions, false, None)
}

/// Replays an exact prefix from a seeded Rust root and reconstructs the Java
/// adapter bindings at that boundary. This lets a branch capture later combat
/// entries without flattening or replaying its pre-branch history.
pub fn replay_recorded_evidence_prefix_from_rust_adapter_for_resume_v1(
    directory: &Path,
    committed_actions: usize,
    rust: RustLockstepAdapterV1,
    java_reload: JavaLockstepResumeStateV1,
) -> Result<
    (
        OfflineReplayReportV1,
        RustLockstepAdapterV1,
        JavaLockstepResumeStateV1,
    ),
    String,
> {
    let evidence = RecordedJavaEvidenceV1::load_prefix(directory, committed_actions)?;
    replay_recorded_evidence_prefix_v1(
        &evidence,
        committed_actions,
        false,
        Some((rust, java_reload)),
    )
}

/// Finds the latest transition from a non-combat boundary into the currently
/// active natural combat, then rebuilds Rust at that entry without touching
/// Java. This is the Rust half of a seeded combat branch.
pub fn replay_latest_natural_combat_entry_into_rust_v1(
    directory: &Path,
) -> Result<
    (
        OfflineReplayReportV1,
        RustLockstepAdapterV1,
        JavaLockstepResumeStateV1,
    ),
    String,
> {
    let evidence = RecordedJavaEvidenceV1::load_complete(directory)?;
    let entry =
        latest_recorded_natural_combat_entry_action_count_v1(&evidence).ok_or_else(|| {
            "recorded trajectory has no typed natural combat entry to retry".to_owned()
        })?;
    replay_recorded_evidence_prefix_v1(&evidence, entry, false, None)
}

/// Returns the committed-action coordinate of the latest observed transition
/// from a non-combat boundary into natural combat Turn 1. Remaining within
/// Turn 1 after playing cards is deliberately not another entry.
pub fn latest_recorded_natural_combat_entry_action_count_v1(
    evidence: &RecordedJavaEvidenceV1,
) -> Option<usize> {
    let observations = std::iter::once(&evidence.initial.observation)
        .chain(evidence.steps.iter().map(|step| &step.after.observation))
        .collect::<Vec<_>>();
    latest_natural_combat_entry_action_count_v1(&observations)
}

pub fn recorded_natural_combat_entry_at_v1(
    evidence: &RecordedJavaEvidenceV1,
    committed_actions: usize,
) -> bool {
    if committed_actions > evidence.steps.len() {
        return false;
    }
    let observations = std::iter::once(&evidence.initial.observation)
        .chain(
            evidence
                .steps
                .iter()
                .take(committed_actions)
                .map(|step| &step.after.observation),
        )
        .collect::<Vec<_>>();
    latest_natural_combat_entry_action_count_v1(&observations) == Some(committed_actions)
}

fn latest_natural_combat_entry_action_count_v1(
    observations: &[&JavaRuntimeObservationV1],
) -> Option<usize> {
    let latest_combat = observations
        .iter()
        .rev()
        .find_map(|observation| natural_combat_run_identity_v1(observation))?;
    observations
        .windows(2)
        .enumerate()
        .rev()
        .find_map(|(index, pair)| {
            (natural_combat_run_identity_v1(pair[1]) == Some(latest_combat)
                && is_natural_combat_entry_transition_v1(pair[0], pair[1]))
            .then_some(index + 1)
        })
}

pub fn is_natural_combat_entry_transition_v1(
    before: &JavaRuntimeObservationV1,
    after: &JavaRuntimeObservationV1,
) -> bool {
    natural_combat_run_identity_v1(after).is_some()
        && natural_combat_turn_v1(after) == Some(1)
        && natural_combat_turn_v1(before).is_none()
}

fn replay_recorded_evidence_prefix_v1(
    evidence: &RecordedJavaEvidenceV1,
    committed_actions: usize,
    require_complete: bool,
    paired_root: Option<(RustLockstepAdapterV1, JavaLockstepResumeStateV1)>,
) -> Result<
    (
        OfflineReplayReportV1,
        RustLockstepAdapterV1,
        JavaLockstepResumeStateV1,
    ),
    String,
> {
    replay_recorded_evidence_observed_v1(
        evidence,
        committed_actions,
        require_complete,
        paired_root,
        &mut |_, _, _| Ok(()),
    )
}

fn replay_recorded_evidence_observed_v1(
    evidence: &RecordedJavaEvidenceV1,
    committed_actions: usize,
    require_complete: bool,
    paired_root: Option<(RustLockstepAdapterV1, JavaLockstepResumeStateV1)>,
    on_exact: &mut dyn FnMut(
        usize,
        &RustLockstepAdapterV1,
        &JavaLockstepAdapterV1<RecordedJavaReplayTransportV1<'_>>,
    ) -> Result<(), String>,
) -> Result<
    (
        OfflineReplayReportV1,
        RustLockstepAdapterV1,
        JavaLockstepResumeStateV1,
    ),
    String,
> {
    replay_recorded_evidence_diagnosed_v1(
        evidence,
        committed_actions,
        require_complete,
        paired_root,
        on_exact,
        None,
    )
}

fn replay_recorded_evidence_diagnosed_v1(
    evidence: &RecordedJavaEvidenceV1,
    committed_actions: usize,
    require_complete: bool,
    paired_root: Option<(RustLockstepAdapterV1, JavaLockstepResumeStateV1)>,
    on_exact: &mut dyn FnMut(
        usize,
        &RustLockstepAdapterV1,
        &JavaLockstepAdapterV1<RecordedJavaReplayTransportV1<'_>>,
    ) -> Result<(), String>,
    mut capture: Option<&mut inspection::TransitionCaptureV1>,
) -> Result<
    (
        OfflineReplayReportV1,
        RustLockstepAdapterV1,
        JavaLockstepResumeStateV1,
    ),
    String,
> {
    let _timing = sts_parity_lockstep::timing_span_v1("replay.verify");
    let preparation_actions = evidence
        .transitions
        .iter()
        .take_while(|transition| {
            transition.action.phase
                == sts_parity_lockstep_java::RecordedJavaActionPhaseV1::NativeReloadSettlement
        })
        .count();
    if preparation_actions > 0 && paired_root.is_none() {
        return Err("native reload preparation requires typed source provenance".to_owned());
    }
    if evidence.native_reload.is_some() && paired_root.is_none() {
        return Err("native Continue execution requires typed source provenance".to_owned());
    }
    let expected_java_actions = preparation_actions
        + replay_prefix_java_action_count_v1(
            &evidence
                .steps
                .iter()
                .map(|step| step.java_actions.len())
                .collect::<Vec<_>>(),
            committed_actions,
        )?;
    let bootstrap = evidence.manifest.bootstrap.clone();
    let config = run_control_config_from_bootstrap_v1(&bootstrap)?;
    let transport = RecordedJavaReplayTransportV1::new(evidence)?;
    let (mut java, mut rust) = match paired_root {
        Some((rust, java_resume)) => {
            // A journal split does not reload the game. Preserve bindings only
            // when the entire raw boundary (including epoch) is identical.
            let java = if java_resume.matches_observation(&evidence.initial.observation) {
                JavaLockstepAdapterV1::resume(transport, bootstrap.clone(), java_resume)?
            } else {
                JavaLockstepAdapterV1::reattach_after_native_reload(
                    transport,
                    bootstrap.clone(),
                    java_resume,
                    evidence.native_reload.clone(),
                )?
            };
            (java, rust)
        }
        None => (
            JavaLockstepAdapterV1::new(transport, bootstrap.clone())?,
            RustLockstepAdapterV1::new(
                RunControlSession::new(config),
                bootstrap.clone(),
                env!("CARGO_PKG_VERSION"),
                "offline-replay",
            )?,
        ),
    };
    let mut rust_current = rust
        .observe()
        .map_err(|failure| format!("Rust initial observation failed: {}", failure.message))?;
    let mut java_current = java
        .observe()
        .map_err(|failure| format!("Java initial observation failed: {}", failure.message))?;
    let mut comparison = compare_snapshots_v1(&java_current, &rust_current)?;
    require_exact("initial boundary", &comparison)?;
    let mut coverage = replay_suite::ReplayCoverageBuilderV1::new(&java_current, &bootstrap)?;
    on_exact(0, &rust, &java)?;

    for (index, step) in evidence.steps.iter().take(committed_actions).enumerate() {
        let action_number = index + 1;
        let mut java_after = None;
        let mut rust_after = None;
        let result = (|| {
            comparison = compare_snapshots_v1(&java_current, &rust_current)?;
            require_exact(&format!("action {action_number} predecessor"), &comparison)?;

            let java_prepared = java
                .prepare(&step.action, &java_current)
                .map_err(|failure| {
                    format!("action {action_number} Java prepare: {}", failure.message)
                })?;
            let rust_prepared = rust
                .prepare(&step.action, &rust_current)
                .map_err(|failure| {
                    format!("action {action_number} Rust prepare: {}", failure.message)
                })?;
            java_after = Some(java.execute(java_prepared).map_err(|failure| {
                format!("action {action_number} Java replay: {}", failure.message)
            })?);
            rust_after = Some(
                rust.execute_with_inputs_v1(rust_prepared, &java.last_execution_inputs_v1())
                    .map_err(|failure| {
                        format!("action {action_number} Rust execute: {}", failure.message)
                    })?,
            );
            let java_rng_calls = java.last_rng_call_ledger_v1().ok_or_else(|| {
                format!("action {action_number} Java replay omitted its committed RNG-call ledger")
            })?;
            let rust_rng_calls = rust.last_rng_call_ledger_v1().ok_or_else(|| {
                format!("action {action_number} Rust replay omitted its committed RNG-call ledger")
            })?;
            let rng_call_comparison =
                compare_natural_run_rng_calls_v1(java_rng_calls, rust_rng_calls)?;
            if let Some(reason) = rng_call_comparison.stop_reason() {
                return Err(format!("action {action_number}: {reason}"));
            }
            comparison =
                compare_snapshots_v1(java_after.as_ref().unwrap(), rust_after.as_ref().unwrap())?;
            require_exact(&format!("action {action_number} successor"), &comparison)?;
            Ok::<_, String>(())
        })();
        if let Some(capture) = capture.as_deref_mut() {
            capture.record(
                index + 1,
                evidence,
                &java_current,
                &rust_current,
                java_after.as_ref(),
                rust_after.as_ref(),
                java_after
                    .as_ref()
                    .and_then(|_| java.last_rng_call_ledger_v1()),
                rust_after
                    .as_ref()
                    .and_then(|_| rust.last_rng_call_ledger_v1()),
                result.as_ref().err(),
            );
        }
        result?;
        coverage.observe_exact(&java_current, &step.action, java_after.as_ref().unwrap())?;
        java_current = java_after.unwrap();
        rust_current = rust_after.unwrap();
        on_exact(index + 1, &rust, &java)?;
    }
    let java_resume = java.resume_state_v1()?;
    let transport = java.into_transport();
    let consumed_java_actions = evidence
        .java_action_count
        .checked_sub(transport.remaining_actions())
        .ok_or_else(|| "offline Java replay consumed more actions than were recorded".to_owned())?;
    if consumed_java_actions != expected_java_actions {
        return Err(format!(
            "offline Java replay consumed {consumed_java_actions} actions for a prefix that records {expected_java_actions}"
        ));
    }
    if require_complete {
        transport.finish()?;
    }

    Ok((
        OfflineReplayReportV1 {
            domains: comparison.domains.len(),
            committed_actions,
            java_actions: consumed_java_actions,
            coverage: coverage.finish(),
        },
        rust,
        java_resume,
    ))
}

fn natural_combat_run_identity_v1(observation: &JavaRuntimeObservationV1) -> Option<(u64, i64)> {
    if natural_combat_turn_v1(observation).is_none()
        || !observation.frame["public_base"]["run"].is_object()
    {
        return None;
    }
    Some((
        observation.frame["public_base"]["run"]["act"].as_u64()?,
        observation.frame["public_base"]["run"]["floor"].as_i64()?,
    ))
}

fn natural_combat_turn_v1(observation: &JavaRuntimeObservationV1) -> Option<u64> {
    // A nested player choice suspends combat; resolving it is not room entry.
    let decision = &observation.frame["public_base"]["decision"];
    let combat_boundary = observation.frame["boundary"]["kind"].as_str()
        == Some("combat_player_decision")
        || (observation.frame["boundary"]["kind"].as_str() == Some("run_decision")
            && (decision["kind"].as_str() == Some("hand_select")
                || (decision["kind"].as_str() == Some("grid_select")
                    && matches!(
                        decision["grid_select"]["source_pile"].as_str(),
                        Some("Draw" | "Exhaust" | "Discard")
                    ))
                || (decision["kind"].as_str() == Some("card_reward")
                    && decision["card_reward"]["source_kind"].as_str()
                        == Some("combat_discovery"))));
    (combat_boundary
        && observation.frame["public_base"]["combat"].is_object()
        && observation.frame["scenario"].is_null())
    .then(|| observation.frame["public_base"]["combat"]["turn"].as_u64())
    .flatten()
}

fn replay_prefix_java_action_count_v1(
    java_actions_per_committed_step: &[usize],
    committed_actions: usize,
) -> Result<usize, String> {
    if committed_actions > java_actions_per_committed_step.len() {
        return Err(format!(
            "requested {committed_actions} committed actions, but recorded evidence contains only {}",
            java_actions_per_committed_step.len()
        ));
    }
    java_actions_per_committed_step[..committed_actions]
        .iter()
        .try_fold(0usize, |total, count| {
            total.checked_add(*count).ok_or_else(|| {
                "recorded Java action count overflowed while selecting replay prefix".to_owned()
            })
        })
}

fn require_exact(label: &str, comparison: &SnapshotComparisonV1) -> Result<(), String> {
    if comparison.status == ComparisonStatusV1::Exact {
        return Ok(());
    }
    let reason = comparison
        .first_stop(LockstepModeV1::Certification)
        .unwrap_or_else(|| "comparison was not exact".to_owned());
    Err(format!("{label}: {reason}"))
}

/// Read-only transport over a complete raw trajectory. Reusing the production
/// Java adapter here keeps combat-birth identity and interstitial settlement
/// stateful without starting a JVM or implementing a second projector.
struct RecordedJavaReplayTransportV1<'a> {
    current: &'a JavaRuntimeObservationV1,
    transitions: &'a [RecordedJavaTransitionV1],
}

enum RecordedSelectionParametersV1<'a> {
    Hand(&'a [String]),
    Grid(&'a [String]),
}

impl<'a> RecordedJavaReplayTransportV1<'a> {
    fn new(evidence: &'a RecordedJavaEvidenceV1) -> Result<Self, String> {
        if evidence.transitions.len() != evidence.java_action_count {
            return Err("recorded Java transition count disagrees with its manifest".to_owned());
        }
        Ok(Self {
            current: &evidence.initial.observation,
            transitions: &evidence.transitions,
        })
    }

    fn finish(self) -> Result<(), String> {
        if self.transitions.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "offline Java adapter left {} recorded actions unconsumed",
                self.transitions.len()
            ))
        }
    }

    fn remaining_actions(&self) -> usize {
        self.transitions.len()
    }

    fn advance(
        &mut self,
        runtime_handle: &str,
        selection: Option<RecordedSelectionParametersV1<'_>>,
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let transition = self.transitions.first().ok_or_else(|| {
            replay_failure("offline Java adapter attempted an action past the recorded trajectory")
        })?;
        if &transition.before.observation != self.current {
            return Err(replay_failure(
                "offline Java transition predecessor differs from the current raw boundary",
            ));
        }
        let matching = self
            .current
            .actions
            .iter()
            .filter(|action| action.runtime_handle == runtime_handle)
            .collect::<Vec<_>>();
        let [runtime_action] = matching.as_slice() else {
            return Err(replay_failure(format!(
                "offline Java runtime handle resolves to {} actions instead of one",
                matching.len()
            )));
        };
        if runtime_action.action_id != transition.action.receipt.action_id
            || transition.action.receipt.based_on_frame_id != self.current.frame_id
            || transition.action.receipt.successor_frame_id
                != Some(transition.after.observation.frame_id)
        {
            return Err(replay_failure(
                "offline Java action does not bind its runtime handle and adjacent boundaries",
            ));
        }

        match selection {
            Some(RecordedSelectionParametersV1::Grid(actual)) => {
                validate_recorded_card_selection_v1(
                    &transition.action.action,
                    self.current,
                    actual,
                    DecisionSurfaceV1::GridSelect,
                )?;
                if !runtime_action.parameters_required {
                    return Err(replay_failure(
                        "offline Java grid parameters differ from the recorded typed selection",
                    ));
                }
            }
            Some(RecordedSelectionParametersV1::Hand(actual)) => {
                validate_recorded_hand_selection_v1(
                    &transition.action.action,
                    self.current,
                    actual,
                )?;
                if !runtime_action.parameters_required {
                    return Err(replay_failure(
                        "offline Java hand selection omitted its runtime parameters",
                    ));
                }
            }
            None => {
                if runtime_action.parameters_required
                    || matches!(
                        transition.action.action,
                        SemanticActionV1::SubmitSelection { .. }
                    )
                {
                    return Err(replay_failure(
                        "offline Java parameterized action was replayed without its selection",
                    ));
                }
            }
        }

        self.transitions = &self.transitions[1..];
        self.current = &transition.after.observation;
        Ok(transition.action.receipt.clone())
    }
}

impl JavaLockstepTransportV1 for RecordedJavaReplayTransportV1<'_> {
    fn status(&mut self) -> Result<JavaRuntimeObservationV1, AdapterFailureV1> {
        Ok(self.current.clone())
    }

    fn act(&mut self, runtime_handle: &str) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        self.advance(runtime_handle, None)
    }

    fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        self.advance(
            runtime_handle,
            Some(RecordedSelectionParametersV1::Grid(selected_card_uuids)),
        )
    }

    fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        self.advance(
            runtime_handle,
            Some(RecordedSelectionParametersV1::Hand(selected_card_uuids)),
        )
    }
}

fn validate_recorded_hand_selection_v1(
    action: &SemanticActionV1,
    observation: &JavaRuntimeObservationV1,
    selected_card_uuids: &[String],
) -> Result<(), AdapterFailureV1> {
    validate_recorded_card_selection_v1(
        action,
        observation,
        selected_card_uuids,
        DecisionSurfaceV1::HandSelect,
    )
}

// The production adapter owns source-proven binding -> UUID resolution,
// including native reload and transforms. This transport only validates the
// resolved parameters against the recorded public selection and typed card
// profile; it must not reconstruct a second binding map from raw deck sources.
fn validate_recorded_card_selection_v1(
    action: &SemanticActionV1,
    observation: &JavaRuntimeObservationV1,
    selected_card_uuids: &[String],
    expected_surface: DecisionSurfaceV1,
) -> Result<(), AdapterFailureV1> {
    let (surface, choices) = match action {
        SemanticActionV1::SubmitSelection { surface, choices } => (*surface, choices.as_slice()),
        // A partial GRID click is one source-bound choice. The production
        // adapter validates its owner and prefix before reaching this transport.
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::GridSelect,
            choice,
        } => (DecisionSurfaceV1::GridSelect, std::slice::from_ref(choice)),
        _ => {
            return Err(replay_failure(
                "offline Java parameterized action is not a recorded card selection",
            ))
        }
    };
    if surface != expected_surface {
        return Err(replay_failure(
            "offline Java selection surface differs from its recorded action",
        ));
    }
    let (field, kind) = match expected_surface {
        DecisionSurfaceV1::HandSelect => ("hand_select", "hand_card"),
        DecisionSurfaceV1::GridSelect => (
            "grid_select",
            sts_parity_lockstep_java::grid_selection_card_kind_v1(observation),
        ),
        _ => {
            return Err(replay_failure(
                "offline Java selection surface is unsupported",
            ))
        }
    };
    if choices.len() != selected_card_uuids.len() {
        return Err(replay_failure(
            "offline Java card selection typed and runtime card counts differ",
        ));
    }
    let candidates = observation.frame["public_base"]["decision"][field]["candidate_cards"]
        .as_array()
        .ok_or_else(|| replay_failure("offline Java card selection omitted its candidates"))?;
    for (index, (choice, selected_uuid)) in choices.iter().zip(selected_card_uuids).enumerate() {
        if choice.kind != kind || (kind != "event_card_offer" && choice.occurrence != 0) {
            return Err(replay_failure(
                "offline Java card selection has an incompatible card identity",
            ));
        }
        if selected_card_uuids[..index].contains(selected_uuid) {
            return Err(replay_failure(
                "offline Java card selection repeats one runtime card",
            ));
        }
        let candidate = candidates
            .iter()
            .find(|card| card["card_uuid"].as_str() == Some(selected_uuid))
            .ok_or_else(|| {
                replay_failure("offline Java card selection runtime card is not a typed candidate")
            })?;
        let card_id = choice.attributes.get("card_id");
        if kind == "event_card_offer"
            && (choice.id != "thelibrary"
                || choice.attributes.get("screen")
                    != Some(&sts_parity_lockstep::SemanticScalarV1::Integer(1))
                || candidate["index"].as_u64() != Some(u64::from(choice.occurrence)))
        {
            return Err(replay_failure(
                "offline Java event selection differs from its offered slot",
            ));
        }
        let upgrades = choice.attributes.get("upgrades");
        if card_id
            != Some(&sts_parity_lockstep::SemanticScalarV1::Text(
                sts_java_parity_protocol::canonical_game_id(
                    candidate["card_id"].as_str().unwrap_or_default(),
                ),
            ))
            || upgrades
                != candidate["upgrades"]
                    .as_i64()
                    .map(sts_parity_lockstep::SemanticScalarV1::Integer)
                    .as_ref()
        {
            return Err(replay_failure(
                "offline Java card selection parameters disagree with the recorded typed card",
            ));
        }
    }
    Ok(())
}

fn replay_failure(message: impl Into<String>) -> AdapterFailureV1 {
    AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message)
}

#[cfg(test)]
mod tests {
    use super::{
        latest_natural_combat_entry_action_count_v1, natural_combat_run_identity_v1,
        natural_combat_turn_v1, replay_prefix_java_action_count_v1,
        validate_recorded_hand_selection_v1,
    };
    use serde_json::json;
    use sts_parity_lockstep::{
        natural_hand_card_choice_v1, DecisionSurfaceV1, ProducerIdentityV1, SemanticActionV1,
    };
    use sts_parity_lockstep_java::JavaRuntimeObservationV1;

    #[test]
    fn offline_grid_parameters_validate_candidates_without_reconstructing_sources() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".into(),
                version: "1".into(),
                session_epoch: "reloaded".into(),
            },
            frame_id: 1,
            frame: json!({
                "public_base": { "decision": { "grid_select": { "candidate_cards": [
                    {"card_uuid":"reloaded-card", "card_id":"Inflame", "upgrades":0},
                    {"card_uuid":"other-card", "card_id":"Bash", "upgrades":0}
                ]}}},
                "parity_internal": { "master_deck": [ {"card_uuid":"reloaded-card", "source":null} ] }
            }),
            actions: Vec::new(),
        };
        let choice =
            sts_parity_lockstep::natural_deck_card_choice_v1("natural_run/starter/0", "inflame", 0)
                .unwrap();
        let action = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices: vec![choice.clone()],
        };
        let click = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::GridSelect,
            choice,
        };
        for recorded in [&action, &click] {
            let validate = |ids: &[String], surface| {
                super::validate_recorded_card_selection_v1(recorded, &observation, ids, surface)
            };
            validate(&["reloaded-card".into()], DecisionSurfaceV1::GridSelect).unwrap();
            assert!(validate(&["other-card".into()], DecisionSurfaceV1::GridSelect).is_err());
            assert!(validate(&["absent-card".into()], DecisionSurfaceV1::GridSelect).is_err());
            assert!(validate(&[], DecisionSurfaceV1::GridSelect).is_err());
            assert!(validate(&["reloaded-card".into()], DecisionSurfaceV1::HandSelect).is_err());
            assert!(validate(
                &["reloaded-card".into(), "other-card".into()],
                DecisionSurfaceV1::GridSelect
            )
            .is_err());
        }

        observation.frame["public_base"]["decision"]["grid_select"]["source_pile"] =
            json!("Exhaust");
        observation.frame["public_base"]["decision"]["grid_select"]["selection_reason"] =
            json!("Exhume");
        let exhume = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices: vec![sts_parity_lockstep::natural_exhaust_card_choice_v1(
                "natural_run/starter/0",
                "inflame",
                0,
            )
            .unwrap()],
        };
        let validate = |action| {
            super::validate_recorded_card_selection_v1(
                action,
                &observation,
                &["reloaded-card".into()],
                DecisionSurfaceV1::GridSelect,
            )
        };
        validate(&exhume).unwrap();
        assert!(validate(&action).is_err());
        for reason in [
            "DiscardToHand",
            "DiscardToHandNoCostChange",
            "DiscardToHandRetain",
            "MoveToDrawPile",
        ] {
            observation.frame["public_base"]["decision"]["grid_select"]["source_pile"] =
                json!("Discard");
            observation.frame["public_base"]["decision"]["grid_select"]["selection_reason"] =
                json!(reason);
            let discard = SemanticActionV1::SubmitSelection {
                surface: DecisionSurfaceV1::GridSelect,
                choices: vec![sts_parity_lockstep::natural_discard_card_choice_v1(
                    "natural_run/starter/0",
                    "inflame",
                    0,
                )
                .unwrap()],
            };
            super::validate_recorded_card_selection_v1(
                &discard,
                &observation,
                &["reloaded-card".into()],
                DecisionSurfaceV1::GridSelect,
            )
            .unwrap();
            assert!(super::validate_recorded_card_selection_v1(
                &action,
                &observation,
                &["reloaded-card".into()],
                DecisionSurfaceV1::GridSelect
            )
            .is_err());
            assert!(super::validate_recorded_card_selection_v1(
                &discard,
                &observation,
                &["other-card".into()],
                DecisionSurfaceV1::GridSelect
            )
            .is_err());
        }
        observation.frame["public_base"]["decision"]["grid_select"]["source_pile"] =
            json!("MasterDeck");
        assert!(super::validate_recorded_card_selection_v1(
            &exhume,
            &observation,
            &["reloaded-card".into()],
            DecisionSurfaceV1::GridSelect,
        )
        .is_err());
    }

    #[test]
    fn offline_draw_selection_requires_matching_source_and_candidate_profile() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".into(),
                version: "1".into(),
                session_epoch: "reloaded".into(),
            },
            frame_id: 1,
            frame: json!({"public_base": {"decision": {"grid_select": {
                "source_pile": "Draw",
                "selection_reason": "SkillFromDeckToHand",
                "candidate_cards": [
                    {"card_uuid":"reloaded-card", "card_id":"Defend_R", "upgrades":0},
                    {"card_uuid":"other-card", "card_id":"Defend_R", "upgrades":1}
                ]
            }}}}),
            actions: Vec::new(),
        };
        let action = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices: vec![sts_parity_lockstep::natural_draw_card_choice_v1(
                "natural_run/starter/5",
                "defend",
                0,
            )
            .unwrap()],
        };
        let validate = |observation: &JavaRuntimeObservationV1, ids: &[String]| {
            super::validate_recorded_card_selection_v1(
                &action,
                observation,
                ids,
                DecisionSurfaceV1::GridSelect,
            )
        };
        let selected = ["reloaded-card".into()];
        validate(&observation, &selected).unwrap();
        assert!(validate(&observation, &["other-card".into()]).is_err());
        assert!(validate(&observation, &["absent-card".into()]).is_err());
        assert!(validate(&observation, &[]).is_err());
        // Card-type legality belongs to the production adapter; the replay
        // transport checks the recorded selection and resolved UUID profile.
        observation.frame["public_base"]["decision"]["grid_select"]["selection_reason"] =
            json!("AttackFromDeckToHand");
        validate(&observation, &selected).unwrap();
        for (pile, reason) in [
            ("MasterDeck", "DeckUpgrade"),
            ("Exhaust", "Exhume"),
            ("Draw", "UnknownReason"),
        ] {
            observation.frame["public_base"]["decision"]["grid_select"]["source_pile"] =
                json!(pile);
            observation.frame["public_base"]["decision"]["grid_select"]["selection_reason"] =
                json!(reason);
            assert!(validate(&observation, &selected).is_err());
        }
    }

    #[test]
    fn offline_discard_selection_requires_matching_source_and_candidate_profile() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".to_owned(),
                version: "1".to_owned(),
                session_epoch: "epoch".to_owned(),
            },
            frame_id: 1,
            frame: json!({"public_base": {"decision": {"grid_select": {
                "source_pile": "Discard", "selection_reason": "DiscardToHand",
                "candidate_cards": [
                    {"card_uuid":"reloaded-card", "card_id":"Defend_R", "upgrades":0},
                    {"card_uuid":"other-card", "card_id":"Defend_R", "upgrades":1}
                ]
            }}}}),
            actions: Vec::new(),
        };
        let action = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices: vec![sts_parity_lockstep::natural_discard_card_choice_v1(
                "natural_run/starter/5",
                "defend",
                0,
            )
            .unwrap()],
        };
        let validate = |observation: &JavaRuntimeObservationV1, ids: &[String]| {
            super::validate_recorded_card_selection_v1(
                &action,
                observation,
                ids,
                DecisionSurfaceV1::GridSelect,
            )
        };
        let selected = ["reloaded-card".into()];
        for reason in [
            "DiscardToHand",
            "DiscardToHandNoCostChange",
            "DiscardToHandRetain",
        ] {
            observation.frame["public_base"]["decision"]["grid_select"]["selection_reason"] =
                json!(reason);
            validate(&observation, &selected).unwrap();
            for ids in [
                vec!["other-card".into()],
                vec!["absent-card".into()],
                vec![],
            ] {
                assert!(validate(&observation, &ids).is_err());
            }
        }
        for (pile, reason) in [
            ("Draw", "SkillFromDeckToHand"),
            ("Exhaust", "Exhume"),
            ("Discard", "UnknownReason"),
        ] {
            observation.frame["public_base"]["decision"]["grid_select"]["source_pile"] =
                json!(pile);
            observation.frame["public_base"]["decision"]["grid_select"]["selection_reason"] =
                json!(reason);
            assert!(validate(&observation, &selected).is_err());
        }
    }

    #[test]
    fn offline_library_selection_requires_the_recorded_offer_slot_and_profile() {
        let observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "fixture".into(),
                version: "1".into(),
                session_epoch: "library".into(),
            },
            frame_id: 1,
            actions: Vec::new(),
            frame: json!({"public_base":{"decision":{"grid_select":{"source_pile":"EventOffer","selection_reason":"TheLibrary","candidate_cards":[
                {"index":7,"card_uuid":"offered","card_id":"True Grit","upgrades":0},
                {"index":8,"card_uuid":"different-slot","card_id":"True Grit","upgrades":0}
            ]}}}}),
        };
        let action = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices: vec![sts_parity_lockstep::natural_event_card_offer_v1(
                "thelibrary",
                1,
                7,
                "truegrit",
                0,
            )
            .unwrap()],
        };
        super::validate_recorded_card_selection_v1(
            &action,
            &observation,
            &["offered".into()],
            DecisionSurfaceV1::GridSelect,
        )
        .unwrap();
        for uuid in ["different-slot", "absent"] {
            assert!(super::validate_recorded_card_selection_v1(
                &action,
                &observation,
                &[uuid.into()],
                DecisionSurfaceV1::GridSelect
            )
            .is_err());
        }
        let mut wrong = observation.clone();
        wrong.frame["public_base"]["decision"]["grid_select"]["candidate_cards"][0]["upgrades"] =
            json!(1);
        assert!(super::validate_recorded_card_selection_v1(
            &action,
            &wrong,
            &["offered".into()],
            DecisionSurfaceV1::GridSelect
        )
        .is_err());
    }

    #[test]
    fn replay_prefix_counts_all_java_settlement_actions_in_selected_steps() {
        assert_eq!(
            replay_prefix_java_action_count_v1(&[1, 3, 1], 2).unwrap(),
            4
        );
        assert_eq!(
            replay_prefix_java_action_count_v1(&[1, 3, 1], 0).unwrap(),
            0
        );
    }

    #[test]
    fn replay_prefix_rejects_a_boundary_past_the_complete_evidence() {
        let error = replay_prefix_java_action_count_v1(&[1, 3, 1], 4).unwrap_err();

        assert!(error.contains("requested 4 committed actions"));
        assert!(error.contains("contains only 3"));
    }

    #[test]
    fn offline_hand_selection_accepts_empty_and_multi_card_submissions() {
        let observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".to_owned(),
                version: "1".to_owned(),
                session_epoch: "epoch".to_owned(),
            },
            frame_id: 1,
            frame: json!({
                "public_base": {
                    "decision": {
                        "hand_select": {
                            "candidate_cards": [
                                {"card_uuid": "u0", "card_id": "True Grit", "upgrades": 1},
                                {"card_uuid": "u1", "card_id": "AscendersBane", "upgrades": 0},
                                {"card_uuid": "u2", "card_id": "Defend_R", "upgrades": 0}
                            ]
                        }
                    }
                }
            }),
            actions: Vec::new(),
        };
        let empty = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::HandSelect,
            choices: Vec::new(),
        };
        validate_recorded_hand_selection_v1(&empty, &observation, &[]).unwrap();

        let selected = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::HandSelect,
            choices: vec![
                natural_hand_card_choice_v1("natural_run/starter/9", "truegrit", 1).unwrap(),
                natural_hand_card_choice_v1("natural_run/starter/10", "ascendersbane", 0).unwrap(),
                natural_hand_card_choice_v1("natural_run/starter/5", "defend", 0).unwrap(),
            ],
        };
        validate_recorded_hand_selection_v1(
            &selected,
            &observation,
            &["u0".to_owned(), "u1".to_owned(), "u2".to_owned()],
        )
        .unwrap();
    }

    #[test]
    fn offline_hand_selection_rejects_cardinality_and_duplicate_runtime_cards() {
        let observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".to_owned(),
                version: "1".to_owned(),
                session_epoch: "epoch".to_owned(),
            },
            frame_id: 1,
            frame: json!({
                "public_base": {
                    "decision": {
                        "hand_select": {
                            "candidate_cards": [
                                {"card_uuid": "u0", "card_id": "Defend_R", "upgrades": 0}
                            ]
                        }
                    }
                }
            }),
            actions: Vec::new(),
        };
        let card = natural_hand_card_choice_v1("natural_run/starter/5", "defend", 0).unwrap();
        let one = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::HandSelect,
            choices: vec![card.clone()],
        };
        assert!(validate_recorded_hand_selection_v1(&one, &observation, &[]).is_err());

        let repeated = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::HandSelect,
            choices: vec![card.clone(), card],
        };
        let error = validate_recorded_hand_selection_v1(
            &repeated,
            &observation,
            &["u0".to_owned(), "u0".to_owned()],
        )
        .unwrap_err();
        assert!(error.message.contains("repeats one runtime card"));
    }

    #[test]
    fn natural_combat_entry_detection_uses_the_raw_boundary_contract() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".to_owned(),
                version: "1".to_owned(),
                session_epoch: "epoch".to_owned(),
            },
            frame_id: 1,
            frame: json!({
                "boundary": {"kind": "combat_player_decision"},
                "public_base": {
                    "run": {"act": 1, "floor": 16},
                    "combat": {"turn": 1},
                    "decision": {}
                },
                "scenario": null
            }),
            actions: Vec::new(),
        };

        assert_eq!(natural_combat_turn_v1(&observation), Some(1));
        assert_eq!(natural_combat_run_identity_v1(&observation), Some((1, 16)));
        observation.frame["scenario"] = json!({"id": "constructed"});
        assert_eq!(natural_combat_turn_v1(&observation), None);
    }

    #[test]
    fn latest_combat_entry_survives_combat_reward_and_map_tails() {
        fn observation(
            frame_id: u64,
            act: u64,
            floor: i64,
            turn: Option<u64>,
        ) -> JavaRuntimeObservationV1 {
            JavaRuntimeObservationV1 {
                producer: ProducerIdentityV1 {
                    implementation: "test".to_owned(),
                    version: "1".to_owned(),
                    session_epoch: "epoch".to_owned(),
                },
                frame_id,
                frame: json!({
                    "boundary": {"kind": if turn.is_some() { "combat_player_decision" } else { "run_decision" }},
                    "public_base": {
                        "run": {"act": act, "floor": floor},
                        "combat": turn.map(|turn| json!({"turn": turn})).unwrap_or(json!(null)),
                        "decision": {}
                    },
                    "scenario": null
                }),
                actions: Vec::new(),
            }
        }

        let map_before = observation(1, 1, 6, None);
        let combat_entry = observation(2, 1, 7, Some(1));
        for decision in [
            json!({"kind": "card_reward", "card_reward": {"source_kind": "combat_discovery"}}),
            json!({"kind": "hand_select"}),
            json!({"kind": "grid_select", "grid_select": {
                "source_pile": "Draw", "selection_reason": "SkillFromDeckToHand"
            }}),
            json!({"kind": "grid_select", "grid_select": {
                "source_pile": "Draw", "selection_reason": "AttackFromDeckToHand"
            }}),
            json!({"kind": "grid_select", "grid_select": {
                "source_pile": "Exhaust", "selection_reason": "Exhume"
            }}),
            json!({"kind": "grid_select", "grid_select": {
                "source_pile": "Discard", "selection_reason": "DiscardToHand"
            }}),
        ] {
            let mut nested = combat_entry.clone();
            nested.frame["boundary"]["kind"] = json!("run_decision");
            nested.frame["public_base"]["decision"] = decision;
            assert_eq!(
                latest_natural_combat_entry_action_count_v1(&[
                    &map_before,
                    &combat_entry,
                    &nested,
                    &combat_entry
                ]),
                Some(1)
            );
            assert_eq!(
                latest_natural_combat_entry_action_count_v1(&[
                    &combat_entry,
                    &nested,
                    &combat_entry
                ]),
                None
            );
        }
        let later_turn = observation(3, 1, 7, Some(5));
        let reward = observation(4, 1, 7, None);
        let map_after = observation(5, 1, 7, None);
        assert_eq!(
            latest_natural_combat_entry_action_count_v1(
                &[&map_before, &combat_entry, &later_turn,]
            ),
            Some(1)
        );
        assert_eq!(
            latest_natural_combat_entry_action_count_v1(&[
                &map_before,
                &combat_entry,
                &later_turn,
                &reward,
            ]),
            Some(1)
        );
        assert_eq!(
            latest_natural_combat_entry_action_count_v1(&[
                &map_before,
                &combat_entry,
                &later_turn,
                &reward,
                &map_after,
            ]),
            Some(1)
        );
    }
}
