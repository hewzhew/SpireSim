//! Derive training calls from the same exact raw-evidence replay.
//! This owns correspondence and acceptance expectations, never game rules.
use crate::RecordedJavaReplayTransportV1;
use crate::{
    manual_replay::ManualReplayVisitorV1, replay_suite::ReplayCoverageBuilderV1, ManualSessionV1,
    OfflineReplayReportV1,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use sts_oracle_learning::{
    ai::planner_core::PublicInformationSnapshotV1,
    eval::run_control::{
        build_decision_surface, learning_public_information_snapshot_with_surface_v1,
        learning_public_selection_snapshot_with_surface_v1,
        run_control_continuation_fingerprint_v1, CombatLearningPotionPolicyV1, LearningBoundaryV1,
        LearningCandidateSurfaceV1, LearningDecisionPathV3, LearningEnvPoolV3, LearningEnvV3,
        LearningExecutionInputsV1, LearningModelChoiceV1, LearningModelDecisionV1,
        LearningPreparedActionV3, LearningSelectionStepV1, RunControlSession,
    },
    state::core::{CampfireChoice, ClientInput, EngineState},
};
use sts_parity_lockstep::{
    DecisionSurfaceV1, LockstepAdapterV1, NaturalRunBootstrapV1, ProducerIdentityV1,
    SemanticActionV1,
};
use sts_parity_lockstep_java::{JavaLockstepAdapterV1, RecordedJavaEvidenceV1};
use sts_parity_lockstep_rust::{run_control_config_from_bootstrap_v1, RustLockstepAdapterV1};

#[derive(Serialize)]
pub struct TrainingTrajectoryV2 {
    pub schema: &'static str,
    pub evidence_directory: PathBuf,
    pub sources: Vec<TrainingTrajectorySourceV1>,
    pub producer_version: &'static str,
    pub bootstrap: NaturalRunBootstrapV1,
    pub candidate_surface: LearningCandidateSurfaceV1,
    pub initial_continuation: String,
    pub initial_public_snapshot: Option<PublicInformationSnapshotV1>,
    pub commits: Vec<TrainingTrajectoryCommitV1>,
    pub replay: OfflineReplayReportV1,
}

#[derive(Serialize)]
pub struct TrainingTrajectoryCommitV1 {
    pub semantic_actions: Vec<usize>,
    pub ordinals: Vec<usize>,
    pub choice_snapshots: Vec<PublicInformationSnapshotV1>,
    pub execution_inputs: LearningExecutionInputsV1,
    pub before_continuation: String,
    pub after_continuation: String,
    pub after_public_snapshot: Option<PublicInformationSnapshotV1>,
    pub reward: i8,
    pub terminated: bool,
}

#[derive(Serialize)]
pub struct TrainingTrajectorySourceV1 {
    pub session_id: Option<String>,
    pub evidence_directory: PathBuf,
    pub first_semantic_action: usize,
    pub java_producer: ProducerIdentityV1,
    pub replay: OfflineReplayReportV1,
}

struct PendingTrainingSource {
    session_id: Option<String>,
    directory: PathBuf,
    producer: ProducerIdentityV1,
}

#[derive(Default)]
struct TrainingRecorder {
    bootstrap: Option<NaturalRunBootstrapV1>,
    pool: Option<LearningEnvPoolV3>,
    initial_continuation: String,
    initial_public_snapshot: Option<PublicInformationSnapshotV1>,
    previous: Option<RustLockstepAdapterV1>,
    staged_smith: Option<usize>,
    staged_astrolabe: Vec<usize>,
    coverage: Option<ReplayCoverageBuilderV1>,
    commits: Vec<TrainingTrajectoryCommitV1>,
    sources: Vec<TrainingTrajectorySourceV1>,
    current: Option<PendingTrainingSource>,
    actions: Vec<SemanticActionV1>,
    offset: usize,
}

impl TrainingRecorder {
    fn begin(
        &mut self,
        session_id: Option<&str>,
        directory: &Path,
        evidence: &RecordedJavaEvidenceV1,
    ) -> Result<(), String> {
        if self.current.is_some() {
            return Err("training replay scope was not closed".into());
        }
        if let Some(bootstrap) = &self.bootstrap {
            if bootstrap != &evidence.manifest.bootstrap {
                return Err("training source bootstrap changed".into());
            }
        } else {
            let config = run_control_config_from_bootstrap_v1(&evidence.manifest.bootstrap)?;
            let pool = LearningEnvPoolV3::from_configs([config])
                .map_err(|e| e.to_string())?
                .with_candidate_surface(LearningCandidateSurfaceV1::Legal);
            self.initial_continuation = pool.continuation_fingerprints_v1().remove(0);
            self.initial_public_snapshot = public_snapshot(pool.boundary(0).unwrap())?;
            self.pool = Some(pool);
            self.bootstrap = Some(evidence.manifest.bootstrap.clone());
        }
        self.offset = self
            .sources
            .iter()
            .map(|source| source.replay.committed_actions)
            .sum();
        self.actions = evidence
            .steps
            .iter()
            .map(|step| step.action.clone())
            .collect();
        self.current = Some(PendingTrainingSource {
            session_id: session_id.map(str::to_owned),
            directory: directory.canonicalize().map_err(|e| e.to_string())?,
            producer: evidence.initial.observation.producer.clone(),
        });
        Ok(())
    }

    fn record(
        &mut self,
        index: usize,
        rust: &RustLockstepAdapterV1,
        java: &JavaLockstepAdapterV1<RecordedJavaReplayTransportV1<'_>>,
    ) -> Result<(), String> {
        let pool = self
            .pool
            .as_mut()
            .ok_or("training replay has no initial source")?;
        if index == 0 {
            if pool.continuation_fingerprints_v1()
                != [run_control_continuation_fingerprint_v1(rust.session())]
            {
                return Err(
                    "training source boundary differs from the verified continuation".into(),
                );
            }
            if self.coverage.is_none() {
                self.coverage = Some(ReplayCoverageBuilderV1::new(
                    &rust.clone().observe().map_err(|e| e.message)?,
                    self.bootstrap.as_ref().unwrap(),
                )?);
            }
            self.previous = Some(rust.clone());
            return Ok(());
        }
        let local_index = index;
        let index = self.offset + index;
        let semantic = &self.actions[local_index - 1];
        let before = self.previous.as_mut().unwrap();
        let snapshot = before.observe().map_err(|e| e.message)?;
        self.coverage.as_mut().unwrap().observe_exact(
            &snapshot,
            semantic,
            &rust.clone().observe().map_err(|e| e.message)?,
        )?;
        let resolved = before.prepare(semantic, &snapshot).map_err(|e| e.message)?;
        let mut input = resolved.resolved_input_v1().cloned();
        let mut grouped_astrolabe = None;
        if let Some((prefix_len, submission, complete)) =
            astrolabe_submission(before.session(), input.as_ref())?
        {
            if self.staged_smith.is_some() || self.staged_astrolabe.len() != prefix_len {
                return Err(format!(
                    "semantic {index}: Astrolabe prefix is not adjacent"
                ));
            }
            if java
                .last_execution_inputs_v1()
                .secret_portal_playtime_bits
                .is_some_and(|values| !values.is_empty())
            {
                return Err(format!(
                    "semantic {index}: Astrolabe click consumed external time"
                ));
            }
            self.staged_astrolabe.push(index);
            if !complete {
                self.previous = Some(rust.clone());
                return Ok(());
            }
            input = Some(submission);
            grouped_astrolabe = Some(std::mem::take(&mut self.staged_astrolabe));
        } else if !self.staged_astrolabe.is_empty() {
            return Err(format!("semantic {index}: interrupted Astrolabe prefix"));
        }
        if input.is_none() {
            if self.staged_smith.is_some() || !is_smith_staging(semantic) {
                return Err(format!("semantic {index}: unpaired UI preparation"));
            }
            if java
                .last_execution_inputs_v1()
                .secret_portal_playtime_bits
                .is_some_and(|values| !values.is_empty())
            {
                return Err(format!(
                    "semantic {index}: Smith UI unexpectedly consumed external time"
                ));
            }
            self.staged_smith = Some(index);
            self.previous = Some(rust.clone());
            return Ok(());
        }
        let env =
            LearningEnvV3::from_checkpoint(pool.checkpoint_slot(0).map_err(|e| e.to_string())?)?;
        let session = env.clone().into_session();
        let mut semantic_actions = grouped_astrolabe.unwrap_or_else(|| vec![index]);
        if let Some(staging) = self.staged_smith.take() {
            if index != staging + 1
                || !matches!(
                    semantic,
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::Campfire,
                        choice,
                    } if choice.kind == "deck_card"
                )
            {
                return Err(format!(
                    "semantic {index}: Smith UI must be followed by its typed card submission"
                ));
            }
            input = Some(smith_input(&session, input.as_ref().unwrap())?);
            semantic_actions.insert(0, staging);
        }
        let boundary = pool.boundary(0).unwrap();
        let decision = LearningModelDecisionV1::from_boundary_with_surface(
            boundary,
            &CombatLearningPotionPolicyV1::All,
            LearningCandidateSurfaceV1::Legal,
        )
        .map_err(|e| e.to_string())?;
        let path = matching_path(
            &env,
            &session,
            &decision,
            input.as_ref().unwrap(),
            resolved.resolved_candidate_id_v1(),
        )?
        .ok_or_else(|| {
            format!("semantic {index}: no unique legal training path for {semantic:?}")
        })?;
        let choice_snapshots = choice_snapshots(boundary, &decision, &path)?;
        let source = pool.decision_token(0).map_err(|e| e.to_string())?;
        let before_continuation = pool.continuation_fingerprints_v1().remove(0);
        let inputs = java.last_execution_inputs_v1();
        let execution_inputs = LearningExecutionInputsV1 {
            secret_portal_playtime_bits: inputs.secret_portal_playtime_bits.unwrap_or_default(),
        };
        let transitions = pool
            .step_active_paths_with_inputs_v1(
                vec![LearningDecisionPathV3 {
                    source,
                    ordinals: path.clone(),
                }],
                vec![execution_inputs.clone()],
            )
            .map_err(|e| format!("semantic {index}: {e}"))?;
        let after_continuation = run_control_continuation_fingerprint_v1(rust.session());
        if pool.continuation_fingerprints_v1() != [after_continuation.clone()] {
            return Err(format!("semantic {index}: training continuation differs"));
        }
        let expected_boundary = LearningEnvV3::from_session(rust.session().clone()).observe()?;
        let after_public_snapshot = public_snapshot(&expected_boundary)?;
        if public_snapshot(pool.boundary(0).unwrap())? != after_public_snapshot {
            return Err(format!(
                "semantic {index}: training public observation differs"
            ));
        }
        let transition = &transitions[0];
        let reward = expected_boundary.terminal_reward();
        if reward != transition.reward
            || expected_boundary.is_terminal() != transition.next.is_none()
        {
            return Err(format!("semantic {index}: reward or termination differs"));
        }
        self.commits.push(TrainingTrajectoryCommitV1 {
            semantic_actions,
            ordinals: path,
            choice_snapshots,
            execution_inputs,
            before_continuation,
            after_continuation,
            after_public_snapshot,
            reward,
            terminated: transition.next.is_none(),
        });
        self.previous = Some(rust.clone());
        Ok(())
    }

    fn end_scope(&mut self, report: &OfflineReplayReportV1) -> Result<(), String> {
        if !self.staged_astrolabe.is_empty() {
            return Err("training scope ends inside an uncommitted Astrolabe selection".into());
        }
        if self.staged_smith.is_some() {
            return Err("training scope ends inside an uncommitted Smith UI choice".into());
        }
        if report.committed_actions != self.actions.len() {
            return Err("training source length differs from exact replay".into());
        }
        let current = self
            .current
            .take()
            .ok_or("training source was not opened")?;
        self.sources.push(TrainingTrajectorySourceV1 {
            session_id: current.session_id,
            evidence_directory: current.directory,
            first_semantic_action: self.offset + 1,
            java_producer: current.producer,
            replay: report.clone(),
        });
        Ok(())
    }

    fn finish(self, directory: &Path) -> Result<TrainingTrajectoryV2, String> {
        if self.current.is_some() || self.sources.is_empty() {
            return Err("training trajectory has an incomplete source".into());
        }
        let domains = self.sources[0].replay.domains;
        if self
            .sources
            .iter()
            .any(|source| source.replay.domains != domains)
        {
            return Err("training source domain scope changed".into());
        }
        let replay = OfflineReplayReportV1 {
            domains,
            committed_actions: self
                .sources
                .iter()
                .map(|source| source.replay.committed_actions)
                .sum(),
            java_actions: self
                .sources
                .iter()
                .map(|source| source.replay.java_actions)
                .sum(),
            coverage: self
                .coverage
                .ok_or("training trajectory has no exact boundary")?
                .finish(),
        };
        Ok(TrainingTrajectoryV2 {
            schema: "sts-training-trajectory-v2",
            evidence_directory: directory.canonicalize().map_err(|e| e.to_string())?,
            sources: self.sources,
            producer_version: env!("CARGO_PKG_VERSION"),
            bootstrap: self.bootstrap.unwrap(),
            candidate_surface: LearningCandidateSurfaceV1::Legal,
            initial_continuation: self.initial_continuation,
            initial_public_snapshot: self.initial_public_snapshot,
            commits: self.commits,
            replay,
        })
    }
}

impl ManualReplayVisitorV1 for TrainingRecorder {
    fn begin_scope(
        &mut self,
        owner: &ManualSessionV1,
        evidence: &RecordedJavaEvidenceV1,
    ) -> Result<(), String> {
        self.begin(
            Some(owner.session_id()),
            &owner.evidence_directory(),
            evidence,
        )
    }
    fn exact(
        &mut self,
        index: usize,
        rust: &RustLockstepAdapterV1,
        java: &JavaLockstepAdapterV1<RecordedJavaReplayTransportV1<'_>>,
    ) -> Result<(), String> {
        self.record(index, rust, java)
    }
    fn finish_scope(&mut self, report: &OfflineReplayReportV1) -> Result<(), String> {
        self.end_scope(report)
    }
}

/// Independently execute public training paths while the common raw replay verifies each transition.
pub fn replay_training_trajectory_v2(directory: &Path) -> Result<TrainingTrajectoryV2, String> {
    let evidence = RecordedJavaEvidenceV1::load_complete(directory)?;
    let mut recorder = TrainingRecorder::default();
    recorder.begin(None, directory, &evidence)?;
    let (report, _, _) = crate::replay_recorded_evidence_observed_v1(
        &evidence,
        evidence.steps.len(),
        true,
        None,
        &mut |index, rust, java| recorder.record(index, rust, java),
    )?;
    recorder.end_scope(&report)?;
    recorder.finish(directory)
}

/// Follow typed parent prefixes through the same source/provenance replay used for native recovery.
pub fn replay_manual_training_trajectory_v2(
    repository: &Path,
    session_id: &str,
    through: Option<usize>,
) -> Result<TrainingTrajectoryV2, String> {
    let session = ManualSessionV1::load_existing(repository, session_id)?;
    let mut recorder = TrainingRecorder::default();
    crate::manual_replay::replay_manual_scope_diagnosed(
        repository,
        &session,
        through,
        None,
        &mut |_| {},
        None,
        Some(&mut recorder),
    )?;
    recorder.finish(&session.evidence_directory())
}

// Original Astrolabe publishes each grid click; the training selection commits
// its ordered three-card resolution once. Keep the clicks as one typed prefix.
fn astrolabe_submission(
    session: &RunControlSession,
    input: Option<&ClientInput>,
) -> Result<Option<(usize, ClientInput, bool)>, String> {
    use sts_oracle_learning::{
        content::relics::RelicId,
        state::core::RunPendingChoiceReason,
        state::selection::{DomainEventSource, SelectionResolution, SelectionScope},
    };
    let EngineState::RunPendingChoice(choice) = &session.engine_state else {
        return Ok(None);
    };
    if choice.source != DomainEventSource::Relic(RelicId::Astrolabe)
        || choice.reason != RunPendingChoiceReason::TransformUpgraded
    {
        return Ok(None);
    }
    let Some(ClientInput::SelectCard(card_index)) = input else {
        return Ok(None);
    };
    if choice.min_choices != 3 || choice.max_choices != 3 || choice.selected.len() >= 3 {
        return Err("Astrolabe training correspondence has an invalid prefix".into());
    }
    let card = session
        .run_state
        .master_deck
        .get(*card_index)
        .ok_or("Astrolabe training click is outside the source deck")?;
    if choice.selected.contains(&card.lineage) {
        return Err("Astrolabe training click repeats a selected lineage".into());
    }
    let mut selected = choice.selected.clone();
    selected.push(card.lineage);
    Ok(Some((
        choice.selected.len(),
        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Deck,
            selected.clone(),
        )),
        selected.len() == 3,
    )))
}

fn is_smith_staging(action: &SemanticActionV1) -> bool {
    matches!(action, SemanticActionV1::Select { surface: DecisionSurfaceV1::Campfire, choice }
        if choice.kind == "campfire_option" && choice.id == "smith" && choice.occurrence == 0)
}

fn smith_input(session: &RunControlSession, input: &ClientInput) -> Result<ClientInput, String> {
    let ClientInput::SubmitSelection(resolution) = input else {
        return Err("Smith requires typed selection".into());
    };
    if resolution.scope != sts_oracle_learning::state::selection::SelectionScope::Deck {
        return Err("Smith requires the master-deck selection scope".into());
    }
    let selected = resolution.selected_card_lineages();
    let [lineage] = selected.as_slice() else {
        return Err("Smith requires exactly one card lineage".into());
    };
    if session.engine_state != EngineState::Campfire {
        return Err("Smith training source is not a campfire".into());
    }
    let index = session
        .run_state
        .master_deck
        .iter()
        .position(|card| card.lineage == *lineage)
        .ok_or("Smith card lineage is absent from the training deck")?;
    Ok(ClientInput::CampfireOption(CampfireChoice::Smith(index)))
}

fn matching_path(
    env: &LearningEnvV3,
    session: &RunControlSession,
    decision: &LearningModelDecisionV1<'_>,
    expected: &ClientInput,
    candidate_id: Option<&str>,
) -> Result<Option<Vec<usize>>, String> {
    let native = build_decision_surface(session);
    let mut matched = None;
    for ordinal in 0..decision.candidates.len() {
        let (path, action) = match decision.choose(ordinal).map_err(|e| e.to_string())? {
            LearningModelChoiceV1::Apply(action) => (vec![ordinal], action),
            LearningModelChoiceV1::DecodeSelection(draft) => {
                let Some((suffix, action)) = draft
                    .ordinal_path_for_input_v1(expected)
                    .map_err(|e| e.to_string())?
                else {
                    continue;
                };
                let mut path = vec![ordinal];
                path.extend(suffix);
                (path, action)
            }
        };
        let matches = match env.prepare_action(action)? {
            LearningPreparedActionV3::StrategicCandidate { run_candidate_id } => {
                candidate_id == Some(run_candidate_id.as_str())
                    || native.view.candidates.iter().any(|candidate| {
                        candidate.id == run_candidate_id
                            && candidate.action.executable_input().as_ref() == Some(expected)
                    })
            }
            LearningPreparedActionV3::RunSelection { resolution, .. } => {
                ClientInput::SubmitSelection(resolution) == *expected
            }
            LearningPreparedActionV3::CombatInput { input } => input == *expected,
        };
        if matches {
            if matched.is_some() {
                return Ok(None);
            }
            matched = Some(path);
        }
    }
    Ok(matched)
}

fn public_snapshot(
    boundary: &LearningBoundaryV1,
) -> Result<Option<PublicInformationSnapshotV1>, String> {
    if boundary.is_terminal() {
        return Ok(None);
    }
    learning_public_information_snapshot_with_surface_v1(
        boundary,
        &CombatLearningPotionPolicyV1::All,
        LearningCandidateSurfaceV1::Legal,
    )
    .map(Some)
}

fn choice_snapshots(
    boundary: &LearningBoundaryV1,
    decision: &LearningModelDecisionV1<'_>,
    path: &[usize],
) -> Result<Vec<PublicInformationSnapshotV1>, String> {
    let mut snapshots = vec![public_snapshot(boundary)?.ok_or("cannot choose at terminal")?];
    if let LearningModelChoiceV1::DecodeSelection(mut draft) =
        decision.choose(path[0]).map_err(|e| e.to_string())?
    {
        for &ordinal in &path[1..] {
            snapshots.push(learning_public_selection_snapshot_with_surface_v1(
                boundary,
                &CombatLearningPotionPolicyV1::All,
                LearningCandidateSurfaceV1::Legal,
                &draft,
            )?);
            if matches!(
                draft.choose(ordinal).map_err(|e| e.to_string())?,
                LearningSelectionStepV1::Apply(_)
            ) {
                break;
            }
        }
    }
    if snapshots.len() != path.len() {
        return Err("training selection path does not end at its commit".into());
    }
    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_learning::{
        eval::run_control::RunControlConfig,
        state::selection::{SelectionResolution, SelectionScope},
    };

    #[test]
    fn astrolabe_clicks_keep_order_and_commit_only_the_complete_selection() {
        use sts_oracle_learning::{
            content::relics::RelicId,
            state::core::{RunPendingChoiceReason, RunPendingChoiceState},
            state::selection::DomainEventSource,
        };
        let mut session = RunControlSession::new(RunControlConfig::default());
        let ids: Vec<_> = session.run_state.master_deck[..3]
            .iter()
            .map(|c| c.lineage)
            .collect();
        session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: vec![],
            min_choices: 3,
            max_choices: 3,
            reason: RunPendingChoiceReason::TransformUpgraded,
            source: DomainEventSource::Relic(RelicId::Astrolabe),
            return_state: Box::new(EngineState::MapNavigation),
        });
        for (prefix, index) in [1, 0, 2].into_iter().enumerate() {
            let (found, input, complete) =
                astrolabe_submission(&session, Some(&ClientInput::SelectCard(index)))
                    .unwrap()
                    .unwrap();
            assert_eq!(found, prefix);
            assert_eq!(complete, prefix == 2);
            let ClientInput::SubmitSelection(selection) = input else {
                panic!("typed selection required")
            };
            let expected: Vec<_> = [1, 0, 2][..=prefix].iter().map(|&i| ids[i]).collect();
            assert_eq!(selection.scope, SelectionScope::Deck);
            assert_eq!(selection.selected_card_lineages(), expected);
            let EngineState::RunPendingChoice(choice) = &mut session.engine_state else {
                unreachable!()
            };
            choice.selected.push(ids[index]);
        }
        assert!(astrolabe_submission(&session, Some(&ClientInput::SelectCard(0))).is_err());
    }

    #[test]
    fn smith_correspondence_uses_the_selected_lineage_and_requires_deck_scope() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::Campfire;
        let lineage = session.run_state.master_deck[0].lineage;
        session.run_state.master_deck.swap(0, 3);
        let selection = |scope, selected| {
            ClientInput::SubmitSelection(SelectionResolution::card_lineages(scope, selected))
        };
        assert_eq!(
            smith_input(&session, &selection(SelectionScope::Deck, vec![lineage])).unwrap(),
            ClientInput::CampfireOption(CampfireChoice::Smith(3))
        );
        assert!(smith_input(&session, &selection(SelectionScope::Hand, vec![lineage])).is_err());
        assert!(smith_input(&session, &selection(SelectionScope::Deck, vec![])).is_err());
        assert!(smith_input(
            &session,
            &selection(SelectionScope::Deck, vec![lineage, lineage])
        )
        .is_err());
        assert!(smith_input(
            &session,
            &selection(
                SelectionScope::Deck,
                vec![sts_oracle_learning::runtime::combat::CardLineageId::master_deck(99)]
            )
        )
        .is_err());
    }
}
