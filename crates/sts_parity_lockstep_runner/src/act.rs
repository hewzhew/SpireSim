//! Continuous certification of a closed adopted Act. Native restoration is
//! preparation evidence; it never occurs inside the certification journal.
use crate::{ManualSessionV1, OfflineReplayReportV1};
use serde::Serialize;
use std::{path::Path, time::Duration};
use sts_parity_lockstep::{
    compare_snapshots_v1, DomainEvidenceV1, LockstepAdapterV1, LockstepModeV1, LockstepRunnerV1,
    NaturalRunBootstrapV1, NaturalRunStateV2, ParityDomainV1, ParitySnapshotV1, ProducerIdentityV1,
    SemanticActionV1,
};
use sts_parity_lockstep_java::{
    JavaLockstepAdapterV1, JavaLockstepTransportV1, JavaRuntimeObservationV1,
    LeasedJavaLockstepTransportV1, RecordedJavaEvidenceStoreV1, RecordedJavaEvidenceV1,
};
use sts_parity_lockstep_rust::{run_control_config_from_bootstrap_v1, RustLockstepAdapterV1};

#[derive(Clone, Debug, Serialize)]
pub struct ActReplaySourceV1 {
    pub session_id: String,
    pub committed_actions: usize,
    pub producer: ProducerIdentityV1,
}

#[derive(Clone, Debug, Serialize)]
struct ActBoundaryRefV1 {
    session_id: String,
    committed_actions: usize,
}

/// Only current-code replay can construct this input. Raw boundary labels
/// locate candidates; complete Java/Rust projections prove both endpoints.
#[derive(Debug, Serialize)]
pub struct VerifiedActReplayV1 {
    schema: &'static str,
    selected_session_id: String,
    act: u8,
    bootstrap: NaturalRunBootstrapV1,
    sources: Vec<ActReplaySourceV1>,
    start: ActBoundaryRefV1,
    end: ActBoundaryRefV1,
    actions: Vec<SemanticActionV1>,
    execution_inputs: Vec<sts_parity_lockstep::ExecutionInputsV1>,
    expected_initial: ParitySnapshotV1,
    expected_terminal: ParitySnapshotV1,
}

impl VerifiedActReplayV1 {
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
    pub fn terminal_label(&self) -> String {
        if self.act == 4 {
            "natural_heart_victory".to_owned()
        } else {
            format!("act{}_map_entrance", self.act + 1)
        }
    }
}

pub fn prepare_manual_act_replay_v1(
    repository: &Path,
    act: u8,
) -> Result<VerifiedActReplayV1, String> {
    if !(1..=4).contains(&act) {
        return Err("Act certification requires an Act from 1 to 4".to_owned());
    }
    let active = ManualSessionV1::load_active(repository)?;
    let mut bootstrap = None;
    let mut sources = Vec::new();
    let mut start = None;
    let mut end = None;
    let mut actions = Vec::new();
    let mut execution_inputs = Vec::new();
    for (owner, limit) in crate::manual_replay::adopted_scopes(repository, &active, None)? {
        let evidence = match limit {
            Some(count) => RecordedJavaEvidenceV1::load_prefix(&owner.evidence_directory(), count)?,
            None => RecordedJavaEvidenceV1::load_complete(&owner.evidence_directory())?,
        };
        if let Some(expected) = &bootstrap {
            if expected != &evidence.manifest.bootstrap {
                return Err("Act replay sources disagree on natural bootstrap".to_owned());
            }
        } else {
            bootstrap = Some(evidence.manifest.bootstrap.clone());
        }
        let reference = |committed_actions| ActBoundaryRefV1 {
            session_id: owner.session_id().to_owned(),
            committed_actions,
        };
        if start.is_none() && entrance_candidate(&evidence.initial.observation, act) {
            start = Some(reference(0));
        }
        let mut consumed = 0;
        for (step, transitions) in evidence.steps.iter().zip(act_step_transitions(&evidence)) {
            consumed += 1;
            if start.is_some() {
                actions.push(step.action.clone());
                execution_inputs.push(
                    sts_parity_lockstep_java::execution_inputs_from_transitions_v1(transitions)?,
                );
                if terminal_candidate(&step.after.observation, act) {
                    end = Some(reference(consumed));
                    break;
                }
            } else if entrance_candidate(&step.after.observation, act) {
                start = Some(reference(consumed));
            }
        }
        sources.push(ActReplaySourceV1 {
            session_id: owner.session_id().to_owned(),
            committed_actions: consumed,
            producer: evidence.initial.observation.producer.clone(),
        });
        if end.is_some() {
            break;
        }
    }
    let start = start.ok_or_else(|| format!("adopted trajectory has no Act {act} entrance"))?;
    let end = end.ok_or_else(|| format!("adopted trajectory has no closed Act {act} endpoint"))?;
    if actions.is_empty() || (act > 1 && start.committed_actions == 0) {
        return Err(
            "Act range requires a nonempty adopted route and a source-proven entrance".to_owned(),
        );
    }
    let bootstrap = bootstrap.ok_or("Act replay has no natural bootstrap")?;
    let mut continuous = replay_boundary(repository, &start)?;
    let expected_initial = continuous.observe().map_err(|e| e.message)?;
    require_act_entrance(&expected_initial, act)?;
    let expected_terminal = replay_boundary(repository, &end)?
        .observe()
        .map_err(|e| e.message)?;
    require_act_terminal(&expected_terminal, act)?;
    // Continuous Rust replay excludes native resets between Audit segments.
    let mut current = expected_initial.clone();
    for (index, action) in actions.iter().enumerate() {
        require_inside_act(&current, act, index)?;
        let prepared = continuous
            .prepare(action, &current)
            .map_err(|e| format!("continuous action {}: {}", index + 1, e.message))?;
        current = continuous
            .execute_with_inputs_v1(prepared, &execution_inputs[index])
            .map_err(|e| format!("continuous action {}: {}", index + 1, e.message))?;
    }
    require_act_terminal(&current, act)?;
    crate::require_exact(
        "continuous Audit terminal",
        &compare_snapshots_v1(&expected_terminal, &current)?,
    )?;
    Ok(VerifiedActReplayV1 {
        schema: "sts-verified-act-replay-input-v1",
        selected_session_id: active.session_id().to_owned(),
        act,
        bootstrap,
        sources,
        start,
        end,
        actions,
        execution_inputs,
        expected_initial,
        expected_terminal,
    })
}

fn act_step_transitions(
    evidence: &RecordedJavaEvidenceV1,
) -> impl Iterator<Item = &[sts_parity_lockstep_java::RecordedJavaTransitionV1]> {
    // Native preparation is present in raw transitions but excluded from
    // committed steps. Each step still owns its interstitial settlement.
    let preparation_count = evidence
        .transitions
        .iter()
        .take_while(|transition| {
            transition.action.phase
                == sts_parity_lockstep_java::RecordedJavaActionPhaseV1::NativeReloadSettlement
        })
        .count();
    evidence
        .steps
        .iter()
        .scan(preparation_count, |offset, step| {
            let start = *offset;
            *offset += step.java_actions.len();
            Some(&evidence.transitions[start..*offset])
        })
}

fn replay_boundary(
    repository: &Path,
    boundary: &ActBoundaryRefV1,
) -> Result<RustLockstepAdapterV1, String> {
    let owner = ManualSessionV1::load_existing(repository, &boundary.session_id)?;
    let (_, rust, _) =
        crate::replay_manual_session_prefix_v1(repository, &owner, boundary.committed_actions)?;
    Ok(rust)
}

fn entrance_candidate(observation: &JavaRuntimeObservationV1, act: u8) -> bool {
    let public = &observation.frame["public_base"];
    public["run"]["act"].as_u64() == Some(u64::from(act))
        && observation.frame["boundary"]["kind"].as_str() == Some("run_decision")
        && if act == 1 {
            public["decision"]["kind"].as_str() == Some("event")
                && public["decision"]["event_id"].as_str() == Some("Neow")
                && matches!(
                    public["decision"]["event_screen_index"].as_u64(),
                    Some(0 | 1)
                )
        } else {
            public["decision"]["kind"].as_str() == Some("map")
        }
}

fn terminal_candidate(observation: &JavaRuntimeObservationV1, act: u8) -> bool {
    if act < 4 {
        entrance_candidate(observation, act + 1)
    } else {
        observation.frame["boundary"]["kind"].as_str() == Some("terminal")
            && observation.frame["public_base"]["decision"]["kind"].as_str()
                == Some("heart_victory")
    }
}

/// Preparation is a separate Audit session. The new Certification journal
/// cannot reload, search, choose alternate actions, or replace the active branch.
pub fn certify_manual_act_v1(
    repository: &Path,
    mut transport: LeasedJavaLockstepTransportV1,
    plan: &VerifiedActReplayV1,
    session: &ManualSessionV1,
    rust_version: &str,
    mut progress: impl FnMut(
        usize,
        &LockstepRunnerV1<
            JavaLockstepAdapterV1<LeasedJavaLockstepTransportV1>,
            RustLockstepAdapterV1,
        >,
    ) -> Result<(), String>,
) -> Result<OfflineReplayReportV1, String> {
    require_selected(repository, plan)?;
    let (java, rust) = if plan.act == 1 {
        let current = transport.status().map_err(|e| e.message)?;
        if current.frame["boundary"]["kind"].as_str() != Some("main_menu_ready") {
            transport
                .discard_run_for_next_case()
                .map_err(|e| e.message)?;
        }
        let token = sts_java_parity_live::java_seed_token_for_engine_seed(&plan.bootstrap.seed)?;
        transport
            .start_run_with_content_profile(
                &plan.bootstrap.player_class,
                u32::from(plan.bootstrap.ascension),
                Some(&token),
                "all_unlocked",
                Duration::from_secs(10),
            )
            .map_err(|e| e.message)?;
        let store = RecordedJavaEvidenceStoreV1::create_new(
            &session.evidence_directory(),
            plan.bootstrap.clone(),
        )?;
        (
            JavaLockstepAdapterV1::with_evidence_store(transport, plan.bootstrap.clone(), store)?,
            fresh_rust(&plan.bootstrap, rust_version)?,
        )
    } else {
        let source = ManualSessionV1::load_existing(repository, &plan.start.session_id)?;
        let recovery = crate::native::prepare_native_recovery_prefix_v1(
            repository,
            &plan.selected_session_id,
            &source,
            plan.start.committed_actions,
        )?;
        let preparation_actions = recovery.suffix_actions();
        let (preparation, restored, _) =
            crate::native::recover_native_for_preparation_v1(repository, transport, recovery)?;
        crate::require_exact(
            "prepared Act entrance",
            &compare_snapshots_v1(&plan.expected_initial, restored.current_snapshots().0)?,
        )?;
        let (java, rust) = restored.into_adapters();
        let resume = java.resume_state_v1()?;
        let store = RecordedJavaEvidenceStoreV1::create_new(
            &session.evidence_directory(),
            plan.bootstrap.clone(),
        )?;
        let java = JavaLockstepAdapterV1::resume_with_evidence_store(
            java.into_transport(),
            plan.bootstrap.clone(),
            store,
            resume,
        )?;
        session.publish_rust_root(
            preparation.session_id(),
            preparation_actions,
            sts_oracle_run_control::eval::run_control::RunControlSessionCheckpointV1::from_session(
                rust.session(),
            ),
        )?;
        crate::manual::write_json_create_new(
            &session.directory().join("certification-preparation.json"),
            &serde_json::json!({
                "schema": "sts-act-certification-preparation-v1",
                "session_id": preparation.session_id(), "excluded_from_certification": true,
                "source_entrance": plan.start,
            }),
        )?;
        (java, rust)
    };
    require_selected(repository, plan)?;
    let (epoch, directory) = session.next_epoch_directory()?;
    let mut runner = LockstepRunnerV1::start_new(
        java,
        rust,
        LockstepModeV1::Certification,
        &directory,
        format!("{}-epoch-{epoch:08}", session.session_id()),
    )?;
    crate::require_exact("Act certification initial", runner.current_comparison())?;
    require_act_entrance(runner.current_snapshots().0, plan.act)?;
    crate::require_exact(
        "adopted Act entrance",
        &compare_snapshots_v1(&plan.expected_initial, runner.current_snapshots().0)?,
    )?;
    // Prove the newly recorded entrance through its actual preparation before
    // committing the Act. In-memory bindings alone cannot certify a journal.
    crate::replay_manual_session_prefix_v1(repository, session, 0)?;
    progress(0, &runner)?;
    for (index, action) in plan.actions.iter().enumerate() {
        require_inside_act(runner.current_snapshots().0, plan.act, index)?;
        runner.step(action.clone())?;
        if runner.is_stopped() {
            return Err(format!(
                "Act certification action {} stopped: {}",
                index + 1,
                runner.stop_reason().unwrap_or("unknown stop")
            ));
        }
        progress(index + 1, &runner)?;
    }
    require_act_terminal(runner.current_snapshots().0, plan.act)?;
    crate::require_exact(
        "certified route terminal",
        &compare_snapshots_v1(&plan.expected_terminal, runner.current_snapshots().0)?,
    )?;
    drop(runner);
    let (report, _, _) = crate::replay_manual_session_v1(repository, session)?;
    if report.committed_actions != plan.action_count() {
        return Err("certification journal does not cover the complete Act".to_owned());
    }
    require_selected(repository, plan)?;
    Ok(report)
}

fn require_selected(repository: &Path, plan: &VerifiedActReplayV1) -> Result<(), String> {
    if ManualSessionV1::load_active(repository)?.session_id() != plan.selected_session_id {
        return Err("selected trajectory changed during Act certification".to_owned());
    }
    Ok(())
}

fn fresh_rust(
    bootstrap: &NaturalRunBootstrapV1,
    version: &str,
) -> Result<RustLockstepAdapterV1, String> {
    RustLockstepAdapterV1::new(
        sts_oracle_run_control::eval::run_control::RunControlSession::new(
            run_control_config_from_bootstrap_v1(bootstrap)?,
        ),
        bootstrap.clone(),
        version,
        "continuous-act",
    )
}

pub(crate) fn run(snapshot: &ParitySnapshotV1) -> Result<NaturalRunStateV2, String> {
    let Some(DomainEvidenceV1::Observed { value, .. }) = snapshot
        .domains
        .get(&ParityDomainV1::Run)
        .map(|domain| &domain.evidence)
    else {
        return Err("Act boundary omitted observed run state".to_owned());
    };
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
}

pub(crate) fn require_act_entrance(snapshot: &ParitySnapshotV1, act: u8) -> Result<(), String> {
    let run = run(snapshot)?;
    if run.act != u64::from(act) {
        return Err(format!("Act certification requires an Act {act} entrance"));
    }
    if act == 1 {
        let introduction = matches!(snapshot.domains.get(&ParityDomainV1::Room).map(|domain| &domain.evidence),
            Some(DomainEvidenceV1::Observed {value, ..})
                if value["kind"] == "neow" && value["stage"] == "introduction");
        if snapshot.boundary_kind != "natural_neow" || run.floor != 0 || !introduction {
            return Err("Act 1 certification requires the natural Neow entrance".to_owned());
        }
    } else {
        let Some(DomainEvidenceV1::Observed { value: map, .. }) = snapshot
            .domains
            .get(&ParityDomainV1::Map)
            .map(|domain| &domain.evidence)
        else {
            return Err("Act boundary omitted observed map".to_owned());
        };
        if snapshot.boundary_kind != "natural_map"
            || map.get("current") != Some(&serde_json::Value::Null)
        {
            return Err(format!(
                "Act {act} entrance requires its map before choosing a node"
            ));
        }
    }
    Ok(())
}

fn require_inside_act(snapshot: &ParitySnapshotV1, act: u8, index: usize) -> Result<(), String> {
    if run(snapshot)?.act != u64::from(act) || snapshot.boundary_kind == "natural_heart_victory" {
        return Err(format!(
            "Act replay action {index} would continue beyond Act {act}"
        ));
    }
    Ok(())
}

pub(crate) fn require_act_terminal(snapshot: &ParitySnapshotV1, act: u8) -> Result<(), String> {
    if act < 4 {
        require_act_entrance(snapshot, act + 1)
    } else if run(snapshot)?.act == 4 && snapshot.boundary_kind == "natural_heart_victory" {
        Ok(())
    } else {
        Err("Act 4 certification must end at terminal Heart victory".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_run_control::{
        eval::run_control::{RunControlConfig, RunControlSession},
        state::core::EngineState,
    };
    use sts_parity_lockstep_rust::natural_run_bootstrap_v1;

    #[test]
    fn act_inputs_keep_committed_settlement_witnesses_after_native_preparation() {
        use sts_parity_lockstep_java::{JavaLockstepReceiptV1, RecordedJavaActionPhaseV1};
        use RecordedJavaActionPhaseV1::{
            Committed, InterstitialSettlement, NativeReloadSettlement,
        };

        for preparation_count in 0..=2 {
            let directory =
                std::env::temp_dir().join(format!("act-input-witnesses-{}", uuid::Uuid::new_v4()));
            let bootstrap = natural_run_bootstrap_v1(&RunControlConfig::default()).unwrap();
            let mut store = RecordedJavaEvidenceStoreV1::create_new(&directory, bootstrap).unwrap();
            let mut observation = JavaRuntimeObservationV1 {
                producer: ProducerIdentityV1 {
                    implementation: "test".into(),
                    version: "1".into(),
                    session_epoch: "epoch".into(),
                },
                frame_id: 1,
                frame: serde_json::json!({}),
                actions: vec![],
            };
            store.append_boundary(&observation).unwrap();
            let mut samples =
                vec![(NativeReloadSettlement, vec![900.0f32.to_bits()]); preparation_count];
            samples.extend([
                (Committed, vec![800.0f32.to_bits()]),
                (InterstitialSettlement, vec![801.0f32.to_bits()]),
                (Committed, vec![]),
            ]);
            for (phase, bits) in samples {
                let from = observation.frame_id;
                let receipt = JavaLockstepReceiptV1 {
                    status: "completed".into(),
                    request_id: format!("request-{from}"),
                    based_on_frame_id: from,
                    action_id: "proceed".into(),
                    successor_frame_id: Some(from + 1),
                };
                store
                    .append_action(
                        phase,
                        &SemanticActionV1::Proceed {
                            surface: sts_parity_lockstep::DecisionSurfaceV1::BossRelic,
                        },
                        &receipt,
                    )
                    .unwrap();
                observation.frame_id += 1;
                observation.frame = serde_json::json!({
                    "previous_frame_id": from,
                    "execution_input_witness": {
                        "kind": "execution_input_witness_v1",
                        "schema": sts_parity_lockstep::EXECUTION_INPUTS_SCHEMA_V1,
                        "from_frame_id": from, "to_frame_id": from + 1,
                        "request_id": receipt.request_id, "action_id": receipt.action_id,
                        "complete": true, "inputs": {"secret_portal_playtime_bits": bits}
                    }
                });
                store.append_boundary(&observation).unwrap();
            }
            let evidence = RecordedJavaEvidenceV1::load_complete(&directory).unwrap();
            let inputs = act_step_transitions(&evidence)
                .map(sts_parity_lockstep_java::execution_inputs_from_transitions_v1)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            std::fs::remove_dir_all(&directory).unwrap();
            assert_eq!(inputs.len(), 2);
            assert_eq!(
                inputs[0].secret_portal_playtime_bits,
                Some(vec![800.0f32.to_bits(), 801.0f32.to_bits()]),
                "native preparation must not shift the committed action's own settlement inputs"
            );
            assert_eq!(
                inputs[1],
                sts_parity_lockstep::ExecutionInputsV1::observed_empty()
            );
        }
    }

    #[test]
    fn act_one_detects_both_original_neow_introduction_screens() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: ProducerIdentityV1 {
                implementation: "test".to_owned(),
                version: "1".to_owned(),
                session_epoch: "epoch".to_owned(),
            },
            frame_id: 1,
            frame: serde_json::json!({
                "boundary": {"kind": "run_decision"},
                "public_base": {"run": {"act": 1, "floor": 0},
                    "decision": {"kind": "event", "event_id": "Neow"}}
            }),
            actions: vec![],
        };
        // NeowEvent uses screen 0 for its first tutorial greeting and screen 1
        // once NEOW_INTRO is disabled. Both precede the reward choice.
        for (screen, expected) in [(0, true), (1, true), (3, false), (99, false)] {
            observation.frame["public_base"]["decision"]["event_screen_index"] =
                serde_json::json!(screen);
            assert_eq!(entrance_candidate(&observation, 1), expected);
        }
    }

    #[test]
    fn act_certificate_requires_natural_entrances_and_rejects_continuation_into_next_act() {
        let config = RunControlConfig {
            seed: 7,
            final_act: true,
            ..Default::default()
        };
        let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let neow = fresh_rust(&bootstrap, "test").unwrap().observe().unwrap();
        require_act_entrance(&neow, 1).unwrap();
        for stage in ["choose_reward", "leave"] {
            let mut later_neow = neow.clone();
            let DomainEvidenceV1::Observed { value, .. } = &mut later_neow
                .domains
                .get_mut(&ParityDomainV1::Room)
                .unwrap()
                .evidence
            else {
                unreachable!()
            };
            value["stage"] = serde_json::json!(stage);
            assert!(require_act_entrance(&later_neow, 1).is_err());
        }
        require_inside_act(&neow, 1, 0).unwrap();
        assert!(require_act_terminal(&neow, 1).is_err());
        let mut session = RunControlSession::new(config);
        for act in 2..=4 {
            session.run_state.advance_act();
            session.run_state.event_state = None;
            session.engine_state = EngineState::MapNavigation;
            let entrance =
                RustLockstepAdapterV1::new(session.clone(), bootstrap.clone(), "test", "next")
                    .unwrap()
                    .observe()
                    .unwrap();
            require_act_entrance(&entrance, act).unwrap();
            require_act_terminal(&entrance, act - 1).unwrap();
            assert!(require_inside_act(&entrance, act - 1, 1).is_err());
            if act == 4 {
                let mut pending = entrance.clone();
                pending.boundary_kind = "natural_true_victory_complete".into();
                assert!(require_act_terminal(&pending, 4).is_err());
                pending.boundary_kind = "natural_heart_victory".into();
                require_act_terminal(&pending, 4).unwrap();
                assert!(require_inside_act(&pending, 4, 1).is_err());
            }
            let mut chosen = entrance;
            let DomainEvidenceV1::Observed { value, .. } = &mut chosen
                .domains
                .get_mut(&ParityDomainV1::Map)
                .unwrap()
                .evidence
            else {
                unreachable!()
            };
            value["current"] = serde_json::json!("F01X00");
            assert!(require_act_entrance(&chosen, act).is_err());
            chosen.domains.remove(&ParityDomainV1::Map);
            assert!(require_act_entrance(&chosen, act).is_err());
        }
    }
}
