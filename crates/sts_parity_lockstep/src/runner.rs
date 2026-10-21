use crate::{
    compare_natural_run_rng_calls_v1, compare_snapshots_v1, ComparisonStatusV1, EndpointV1,
    ExecutionInputsV1, LockstepJournalStoreV1, LockstepModeV1, NaturalRunRngCallComparisonV1,
    NaturalRunRngCallLedgerV1, ParityDomainV1, ParitySnapshotV1, SemanticActionSpaceV1,
    SemanticActionV1, SnapshotComparisonV1,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterFailureKindV1 {
    Unsupported,
    Unobserved,
    Rejected,
    Transport,
    Evidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterFailureV1 {
    pub kind: AdapterFailureKindV1,
    pub message: String,
}

impl AdapterFailureV1 {
    pub fn new(kind: AdapterFailureKindV1, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Adapter preparation must be read-only. It resolves one semantic action to
/// runtime-local handles before either endpoint is allowed to mutate.
pub trait LockstepAdapterV1 {
    type PreparedAction;

    fn observe(&mut self) -> Result<ParitySnapshotV1, AdapterFailureV1>;

    fn prepare(
        &mut self,
        action: &SemanticActionV1,
        based_on: &ParitySnapshotV1,
    ) -> Result<Self::PreparedAction, AdapterFailureV1>;

    fn execute(
        &mut self,
        prepared: Self::PreparedAction,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1>;

    /// RNG calls made by the most recent successful committed execution.
    /// Preparation and observation must never populate this ledger.
    fn last_rng_call_ledger_v1(&self) -> Option<&NaturalRunRngCallLedgerV1>;

    fn last_execution_inputs_v1(&self) -> ExecutionInputsV1 {
        ExecutionInputsV1::default()
    }

    fn execute_with_inputs_v1(
        &mut self,
        prepared: Self::PreparedAction,
        inputs: &ExecutionInputsV1,
    ) -> Result<ParitySnapshotV1, AdapterFailureV1> {
        inputs
            .validate()
            .map_err(|message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message))?;
        if inputs.has_samples() {
            return Err(AdapterFailureV1::new(
                AdapterFailureKindV1::Unsupported,
                "adapter cannot consume environmental execution inputs",
            ));
        }
        self.execute(prepared)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterPhaseV1 {
    Prepare,
    Execute,
    ValidateSuccessor,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TransitionOutcomeV1 {
    Continued {
        with_coverage_gap: bool,
    },
    StoppedByComparison {
        reason: String,
    },
    StoppedByAdapter {
        endpoint: EndpointV1,
        phase: AdapterPhaseV1,
        failure: AdapterFailureV1,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionJournalEntryV1 {
    pub schema: String,
    pub index: u64,
    pub action: SemanticActionV1,
    pub java_before: ParitySnapshotV1,
    pub rust_before: ParitySnapshotV1,
    pub java_after: Option<ParitySnapshotV1>,
    pub rust_after: Option<ParitySnapshotV1>,
    pub comparison: Option<SnapshotComparisonV1>,
    pub rng_call_comparison: Option<NaturalRunRngCallComparisonV1>,
    #[serde(default)]
    pub execution_inputs: Option<ExecutionInputsV1>,
    pub outcome: TransitionOutcomeV1,
}

pub struct LockstepRunnerV1<J, R> {
    java: J,
    rust: R,
    mode: LockstepModeV1,
    journal: LockstepJournalStoreV1,
    java_current: ParitySnapshotV1,
    rust_current: ParitySnapshotV1,
    current_comparison: SnapshotComparisonV1,
    next_step: u64,
    stopped: bool,
    stop_reason: Option<String>,
    last_transition_outcome: Option<TransitionOutcomeV1>,
    view_owner: String,
}

impl<J, R> LockstepRunnerV1<J, R>
where
    J: LockstepAdapterV1,
    R: LockstepAdapterV1,
{
    pub fn start_new(
        mut java: J,
        mut rust: R,
        mode: LockstepModeV1,
        journal_directory: &Path,
        session_id: impl Into<String>,
    ) -> Result<Self, String> {
        let java_initial = java
            .observe()
            .map_err(|failure| format!("Java initial observation failed: {}", failure.message))?;
        let rust_initial = rust
            .observe()
            .map_err(|failure| format!("Rust initial observation failed: {}", failure.message))?;
        let comparison = compare_snapshots_v1(&java_initial, &rust_initial)?;
        let stopped = !comparison.may_continue(mode);
        let stop_reason = stopped.then(|| comparison_stop_reason(&comparison, mode));
        let journal = LockstepJournalStoreV1::create_new(
            journal_directory,
            session_id,
            mode,
            java_initial.clone(),
            rust_initial.clone(),
            comparison.clone(),
        )?;
        Ok(Self {
            view_owner: journal
                .directory()
                .canonicalize()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned(),
            java,
            rust,
            mode,
            journal,
            java_current: java_initial,
            rust_current: rust_initial,
            current_comparison: comparison,
            next_step: 0,
            stopped,
            stop_reason,
            last_transition_outcome: None,
        })
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    pub fn current_comparison(&self) -> &SnapshotComparisonV1 {
        &self.current_comparison
    }

    pub fn current_snapshots(&self) -> (&ParitySnapshotV1, &ParitySnapshotV1) {
        (&self.java_current, &self.rust_current)
    }

    /// Borrow the Rust endpoint for read-only private planning.
    ///
    /// A planned action still has to pass through [`Self::step`], so this does
    /// not let callers mutate either endpoint or bypass lockstep comparison.
    pub fn rust_adapter(&self) -> &R {
        &self.rust
    }

    /// Inspect the observed oracle without executing actions outside lockstep.
    pub fn java_adapter(&self) -> &J {
        &self.java
    }

    /// Return both endpoint adapters after a bounded run has finished.
    ///
    /// This is lifecycle plumbing only: callers cannot bypass comparison while
    /// the runner owns the adapters, but a resident endpoint can be returned to
    /// its host after the journaled case is complete.
    pub fn into_adapters(self) -> (J, R) {
        (self.java, self.rust)
    }

    pub fn stop_reason(&self) -> Option<&str> {
        self.stop_reason.as_deref()
    }

    /// The outcome of the last durably recorded transition, without parsing
    /// diagnostic messages. An incomplete journal write has no new outcome.
    pub fn last_transition_outcome(&self) -> Option<&TransitionOutcomeV1> {
        self.last_transition_outcome.as_ref()
    }

    pub fn current_action_space(&self) -> Result<SemanticActionSpaceV1, String> {
        Ok(self.current_action_view()?.actions().clone())
    }

    pub fn current_action_view(&self) -> Result<crate::ActionViewV1, String> {
        let comparison = self
            .current_comparison
            .domains
            .iter()
            .find(|domain| domain.domain == ParityDomainV1::ActionSpace)
            .ok_or_else(|| "current lockstep contract has no action-space domain".to_owned())?;
        if comparison.status != ComparisonStatusV1::Exact {
            return Err(comparison
                .reason
                .clone()
                .unwrap_or_else(|| "current action spaces are not exact".to_owned()));
        }
        crate::ActionViewV1::new(&self.view_owner, self.next_step, &self.java_current)
    }

    pub fn resolve_action_handle(&self, handle: &str) -> Result<SemanticActionV1, String> {
        if self.stopped {
            return Err(self
                .stop_reason
                .clone()
                .unwrap_or_else(|| "lockstep runner is stopped".to_owned()));
        }
        let view = self.current_action_view()?;
        view.resolve_for(&view, handle)
    }

    /// Shared executor for already verified finite trajectories. Source and
    /// terminal gates remain with the proposal owner; each step still journals.
    pub fn commit_exact_actions(
        &mut self,
        actions: &[SemanticActionV1],
        mut on_exact: impl FnMut(usize, &Self) -> Result<(), String>,
    ) -> Result<usize, String> {
        if self.stopped || self.current_comparison.status != ComparisonStatusV1::Exact {
            return Err("trajectory adoption requires an Exact running boundary".into());
        }
        for (index, action) in actions.iter().enumerate() {
            self.step(action.clone())?;
            if self.stopped {
                return Err(format!(
                    "action {} stopped: {}",
                    index + 1,
                    self.stop_reason().unwrap_or("unknown")
                ));
            }
            if self.current_comparison.status != ComparisonStatusV1::Exact {
                return Err(format!(
                    "action {} has a coverage gap; trajectory adoption stopped",
                    index + 1
                ));
            }
            on_exact(index + 1, self)?;
        }
        Ok(actions.len())
    }

    pub fn step_handle(&mut self, handle: &str) -> Result<TransitionJournalEntryV1, String> {
        let action = self.resolve_action_handle(handle)?;
        self.step(action)
    }

    pub fn step(&mut self, action: SemanticActionV1) -> Result<TransitionJournalEntryV1, String> {
        let _timing = crate::timing_span_v1("lockstep.step");
        if self.stopped {
            return Err(self
                .stop_reason
                .clone()
                .unwrap_or_else(|| "lockstep runner is stopped".to_owned()));
        }
        action.validate()?;

        let java_prepared = match self.java.prepare(&action, &self.java_current) {
            Ok(value) => value,
            Err(failure) => {
                return self.stop_adapter(
                    action,
                    EndpointV1::Java,
                    AdapterPhaseV1::Prepare,
                    failure,
                )
            }
        };
        let rust_prepared = match self.rust.prepare(&action, &self.rust_current) {
            Ok(value) => value,
            Err(failure) => {
                return self.stop_adapter(
                    action,
                    EndpointV1::Rust,
                    AdapterPhaseV1::Prepare,
                    failure,
                )
            }
        };

        let java_after = match self.java.execute(java_prepared) {
            Ok(value) => value,
            Err(failure) => {
                return self.stop_adapter(
                    action,
                    EndpointV1::Java,
                    AdapterPhaseV1::Execute,
                    failure,
                )
            }
        };
        if let Err(message) = validate_successor(&self.java_current, &java_after) {
            return self.stop_adapter_with_java(
                action,
                java_after,
                EndpointV1::Java,
                AdapterPhaseV1::ValidateSuccessor,
                AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message),
            );
        }
        let Some(java_rng_calls) = self.java.last_rng_call_ledger_v1().cloned() else {
            return self.stop_adapter_with_java(
                action,
                java_after,
                EndpointV1::Java,
                AdapterPhaseV1::ValidateSuccessor,
                AdapterFailureV1::new(
                    AdapterFailureKindV1::Evidence,
                    "Java adapter omitted the committed RNG-call ledger",
                ),
            );
        };
        if let Err(message) = java_rng_calls.validate() {
            return self.stop_adapter_with_java(
                action,
                java_after,
                EndpointV1::Java,
                AdapterPhaseV1::ValidateSuccessor,
                AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message),
            );
        }

        let execution_inputs = self.java.last_execution_inputs_v1();
        let rust_after = match self
            .rust
            .execute_with_inputs_v1(rust_prepared, &execution_inputs)
        {
            Ok(value) => value,
            Err(failure) => {
                return self.stop_adapter_with_java(
                    action,
                    java_after,
                    EndpointV1::Rust,
                    AdapterPhaseV1::Execute,
                    failure,
                )
            }
        };
        if let Err(message) = validate_successor(&self.rust_current, &rust_after) {
            return self.stop_adapter_with_java(
                action,
                java_after,
                EndpointV1::Rust,
                AdapterPhaseV1::ValidateSuccessor,
                AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message),
            );
        }
        let Some(rust_rng_calls) = self.rust.last_rng_call_ledger_v1().cloned() else {
            return self.stop_adapter_with_both(
                action,
                java_after,
                rust_after,
                EndpointV1::Rust,
                AdapterPhaseV1::ValidateSuccessor,
                AdapterFailureV1::new(
                    AdapterFailureKindV1::Evidence,
                    "Rust adapter omitted the committed RNG-call ledger",
                ),
            );
        };
        if let Err(message) = rust_rng_calls.validate() {
            return self.stop_adapter_with_both(
                action,
                java_after,
                rust_after,
                EndpointV1::Rust,
                AdapterPhaseV1::ValidateSuccessor,
                AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message),
            );
        }

        let comparison = compare_snapshots_v1(&java_after, &rust_after)?;
        let rng_call_comparison =
            compare_natural_run_rng_calls_v1(&java_rng_calls, &rust_rng_calls)?;
        let may_continue = comparison.may_continue(self.mode) && rng_call_comparison.exact;
        let stop_reason = (!may_continue).then(|| {
            rng_call_comparison
                .stop_reason()
                .unwrap_or_else(|| comparison_stop_reason(&comparison, self.mode))
        });
        let outcome = if may_continue {
            TransitionOutcomeV1::Continued {
                with_coverage_gap: comparison.status == crate::ComparisonStatusV1::Unobserved,
            }
        } else {
            TransitionOutcomeV1::StoppedByComparison {
                reason: stop_reason
                    .clone()
                    .expect("non-continuing comparison has a stop reason"),
            }
        };
        let entry = TransitionJournalEntryV1 {
            schema: "sts-lockstep-transition-detail-v3".to_owned(),
            index: self.next_step,
            action,
            java_before: self.java_current.clone(),
            rust_before: self.rust_current.clone(),
            java_after: Some(java_after.clone()),
            rust_after: Some(rust_after.clone()),
            comparison: Some(comparison.clone()),
            rng_call_comparison: Some(rng_call_comparison),
            execution_inputs: Some(execution_inputs),
            outcome,
        };
        // Once either endpoint has mutated, journal failure must freeze the
        // runner instead of allowing an unrecorded transition to continue.
        self.stopped = true;
        self.last_transition_outcome = None;
        if let Err(error) = self.journal.append(&entry) {
            self.stop_reason = Some(format!("lockstep journal append failed: {error}"));
            return Err(error);
        }
        self.java_current = java_after;
        self.rust_current = rust_after;
        self.current_comparison = comparison;
        self.advance_index()?;
        self.stopped = !may_continue;
        self.stop_reason = stop_reason;
        self.last_transition_outcome = Some(entry.outcome.clone());
        Ok(entry)
    }

    fn stop_adapter(
        &mut self,
        action: SemanticActionV1,
        endpoint: EndpointV1,
        phase: AdapterPhaseV1,
        failure: AdapterFailureV1,
    ) -> Result<TransitionJournalEntryV1, String> {
        self.stop_adapter_parts(action, None, None, endpoint, phase, failure)
    }

    fn stop_adapter_with_java(
        &mut self,
        action: SemanticActionV1,
        java_after: ParitySnapshotV1,
        endpoint: EndpointV1,
        phase: AdapterPhaseV1,
        failure: AdapterFailureV1,
    ) -> Result<TransitionJournalEntryV1, String> {
        self.stop_adapter_parts(action, Some(java_after), None, endpoint, phase, failure)
    }

    fn stop_adapter_with_both(
        &mut self,
        action: SemanticActionV1,
        java_after: ParitySnapshotV1,
        rust_after: ParitySnapshotV1,
        endpoint: EndpointV1,
        phase: AdapterPhaseV1,
        failure: AdapterFailureV1,
    ) -> Result<TransitionJournalEntryV1, String> {
        self.stop_adapter_parts(
            action,
            Some(java_after),
            Some(rust_after),
            endpoint,
            phase,
            failure,
        )
    }

    fn stop_adapter_parts(
        &mut self,
        action: SemanticActionV1,
        java_after: Option<ParitySnapshotV1>,
        rust_after: Option<ParitySnapshotV1>,
        endpoint: EndpointV1,
        phase: AdapterPhaseV1,
        failure: AdapterFailureV1,
    ) -> Result<TransitionJournalEntryV1, String> {
        let stop_reason = format!(
            "{} {} failed: {}",
            endpoint_label(endpoint),
            phase_label(phase),
            failure.message
        );
        let entry = TransitionJournalEntryV1 {
            schema: "sts-lockstep-transition-detail-v3".to_owned(),
            index: self.next_step,
            action,
            java_before: self.java_current.clone(),
            rust_before: self.rust_current.clone(),
            execution_inputs: java_after
                .as_ref()
                .map(|_| self.java.last_execution_inputs_v1()),
            java_after,
            rust_after,
            comparison: None,
            rng_call_comparison: None,
            outcome: TransitionOutcomeV1::StoppedByAdapter {
                endpoint,
                phase,
                failure,
            },
        };
        self.stopped = true;
        self.stop_reason = Some(stop_reason);
        self.last_transition_outcome = None;
        self.journal.append(&entry)?;
        self.advance_index()?;
        self.last_transition_outcome = Some(entry.outcome.clone());
        Ok(entry)
    }

    fn advance_index(&mut self) -> Result<(), String> {
        self.next_step = self
            .next_step
            .checked_add(1)
            .ok_or_else(|| "lockstep runner step overflowed u64".to_owned())?;
        Ok(())
    }
}

fn comparison_stop_reason(comparison: &SnapshotComparisonV1, mode: LockstepModeV1) -> String {
    comparison
        .first_stop(mode)
        .unwrap_or_else(|| "lockstep comparison stopped".to_owned())
}

const fn endpoint_label(endpoint: EndpointV1) -> &'static str {
    match endpoint {
        EndpointV1::Java => "Java",
        EndpointV1::Rust => "Rust",
    }
}

const fn phase_label(phase: AdapterPhaseV1) -> &'static str {
    match phase {
        AdapterPhaseV1::Prepare => "prepare",
        AdapterPhaseV1::Execute => "execute",
        AdapterPhaseV1::ValidateSuccessor => "successor validation",
    }
}

fn validate_successor(before: &ParitySnapshotV1, after: &ParitySnapshotV1) -> Result<(), String> {
    after.validate()?;
    if after.producer != before.producer {
        return Err("endpoint producer identity changed during a committed transition".to_owned());
    }
    if after.observation_id == before.observation_id {
        return Err("endpoint reused its predecessor observation identity".to_owned());
    }
    Ok(())
}

pub fn recompare_recorded_java_snapshot_after_v1(
    entry: &TransitionJournalEntryV1,
    corrected_rust_after: &ParitySnapshotV1,
) -> Result<SnapshotComparisonV1, String> {
    let java_after = entry
        .java_after
        .as_ref()
        .ok_or_else(|| "recorded transition has no Java successor".to_owned())?;
    compare_snapshots_v1(java_after, corrected_rust_after)
}
