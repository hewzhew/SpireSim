use crate::RecordedJavaReplayTransportV1;
use crate::{
    progress::progress_phase, LockstepProgressV1, ManualRustRootV1, ManualSessionV1,
    OfflineReplayReportV1,
};
use std::path::Path;
use sts_parity_lockstep_java::{
    JavaLockstepAdapterV1, JavaLockstepResumeStateV1, RecordedJavaEvidenceV1,
};
use sts_parity_lockstep_rust::RustLockstepAdapterV1;

pub(crate) type ReplayedSession = (
    OfflineReplayReportV1,
    RustLockstepAdapterV1,
    JavaLockstepResumeStateV1,
);

pub(crate) trait ManualReplayVisitorV1 {
    fn begin_scope(
        &mut self,
        owner: &ManualSessionV1,
        evidence: &RecordedJavaEvidenceV1,
    ) -> Result<(), String>;
    fn exact(
        &mut self,
        index: usize,
        rust: &RustLockstepAdapterV1,
        java: &JavaLockstepAdapterV1<RecordedJavaReplayTransportV1<'_>>,
    ) -> Result<(), String>;
    fn finish_scope(&mut self, report: &OfflineReplayReportV1) -> Result<(), String>;
}

pub fn replay_manual_session_v1(
    repository: &Path,
    session: &ManualSessionV1,
) -> Result<ReplayedSession, String> {
    replay_manual_session_with_progress_v1(repository, session, &mut |_| {})
}

pub fn replay_manual_session_with_progress_v1(
    repository: &Path,
    session: &ManualSessionV1,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<ReplayedSession, String> {
    replay_manual_scope_observed(repository, session, None, None, observer)
        .map(|(terminal, _)| terminal)
}

pub fn replay_manual_session_prefix_v1(
    repository: &Path,
    session: &ManualSessionV1,
    committed_actions: usize,
) -> Result<ReplayedSession, String> {
    replay_manual_scope_observed(
        repository,
        session,
        Some(committed_actions),
        None,
        &mut |_| {},
    )
    .map(|(terminal, _)| terminal)
}

pub(crate) fn adopted_scopes(
    repository: &Path,
    session: &ManualSessionV1,
    limit: Option<usize>,
) -> Result<Vec<(ManualSessionV1, Option<usize>)>, String> {
    let mut scopes = Vec::<(ManualSessionV1, Option<usize>)>::new();
    let mut current = session.clone();
    let mut limit = limit;
    loop {
        if scopes.len() >= 64
            || scopes
                .iter()
                .any(|(owner, _)| owner.session_id() == current.session_id())
        {
            return Err("manual seeded checkpoint provenance is cyclic or too deep".to_owned());
        }
        let root = current.load_source_prefix()?;
        scopes.push((current, limit));
        let Some(root) = root else { break };
        current = ManualSessionV1::load_existing(repository, &root.source_session_id)?;
        limit = Some(root.source_committed_actions);
    }
    scopes.reverse();
    Ok(scopes)
}

/// The retained pair is process-local and returned only if the entire selected
/// scope succeeds. It cannot be deserialized or substitute for raw provenance.
pub(crate) fn replay_manual_scope_observed(
    repository: &Path,
    session: &ManualSessionV1,
    limit: Option<usize>,
    anchor: Option<(&str, usize)>,
    observer: &mut dyn FnMut(LockstepProgressV1),
) -> Result<
    (
        ReplayedSession,
        Option<Box<(RustLockstepAdapterV1, JavaLockstepResumeStateV1)>>,
    ),
    String,
> {
    replay_manual_scope_diagnosed(repository, session, limit, anchor, observer, None, None)
}

pub(crate) fn replay_manual_scope_diagnosed(
    repository: &Path,
    session: &ManualSessionV1,
    limit: Option<usize>,
    anchor: Option<(&str, usize)>,
    observer: &mut dyn FnMut(LockstepProgressV1),
    mut inspection: Option<&mut crate::inspection::ManualTransitionInspectionV1>,
    mut visitor: Option<&mut dyn ManualReplayVisitorV1>,
) -> Result<
    (
        ReplayedSession,
        Option<Box<(RustLockstepAdapterV1, JavaLockstepResumeStateV1)>>,
    ),
    String,
> {
    let _timing = sts_parity_lockstep::timing_span_v1("replay.sources");
    progress_phase(observer, "offline trajectory verification", |observer| {
        // Keep only one live adapter pair on the stack. Recursive provenance replay
        // accumulated large adapter/result frames and overflowed on valid branches.
        let mut replayed = None;
        let mut captured = None;
        let scopes = adopted_scopes(repository, session, limit)?;
        if let Some((id, count)) = anchor {
            let (_, limit) = scopes
                .iter()
                .find(|(owner, _)| owner.session_id() == id)
                .ok_or("native anchor is outside the selected trajectory")?;
            if limit.is_some_and(|limit| count > limit) {
                return Err("native anchor lies beyond the adopted source prefix".to_owned());
            }
        }
        for (index, (owner, limit)) in scopes.iter().enumerate() {
            let started = std::time::Instant::now();
            let mut capture = crate::inspection::TransitionCaptureV1::new(
                owner.session_id(),
                inspection.as_ref().map(|report| &report.requested),
            );
            let mut completed_actions = 0;
            let mut recorded_actions = None;
            let mut loaded_ms = None;
            let next = (|| {
                let evidence = match limit {
                    None => RecordedJavaEvidenceV1::load_complete(&owner.evidence_directory())?,
                    Some(count) => {
                        RecordedJavaEvidenceV1::load_prefix(&owner.evidence_directory(), *count)?
                    }
                };
                let count = evidence.steps.len();
                recorded_actions = Some(count);
                loaded_ms = Some(started.elapsed().as_millis());
                if let Some(visitor) = visitor.as_deref_mut() {
                    visitor.begin_scope(owner, &evidence)?;
                }
                let next = crate::replay_recorded_evidence_diagnosed_v1(
                    &evidence,
                    count,
                    limit.is_none(),
                    replayed.map(|(_, rust, java_resume)| (rust, java_resume)),
                    &mut |completed, rust, java| {
                        completed_actions = completed;
                        if let Some(visitor) = visitor.as_deref_mut() {
                            visitor.exact(completed, rust, java)?;
                        }
                        if anchor == Some((owner.session_id(), completed)) {
                            captured = Some(Box::new((rust.clone(), java.resume_state_v1()?)));
                        }
                        observer(LockstepProgressV1::Replay {
                            scope: index + 1,
                            scopes: scopes.len(),
                            completed,
                            total: count,
                        });
                        Ok(())
                    },
                    inspection.as_ref().map(|_| &mut capture),
                )?;
                if limit.is_some_and(|count| next.0.committed_actions != count) {
                    return Err("manual combat root disagrees with its source evidence".to_owned());
                }
                if let Some(visitor) = visitor.as_deref_mut() {
                    visitor.finish_scope(&next.0)?;
                }
                Ok::<_, String>(next)
            })();
            if let Some(report) = inspection.as_deref_mut() {
                report.finish_scope(
                    owner.session_id(),
                    recorded_actions,
                    completed_actions,
                    loaded_ms,
                    started.elapsed().as_millis(),
                    capture,
                    next.as_ref().err(),
                );
            }
            replayed = Some(next?);
        }
        // Rebuild every declared prefix with current simulator semantics and raw
        // Java evidence. Captured Rust checkpoints remain diagnostic; each child's
        // initial boundary is independently compared, including reload identity.
        if anchor.is_some() && captured.is_none() {
            return Err("native anchor lies beyond the adopted source prefix".to_owned());
        }
        Ok((
            replayed.ok_or_else(|| "manual replay has no source scope".to_owned())?,
            captured,
        ))
    })
}

pub fn replay_manual_combat_root_v1(
    repository: &Path,
    root: &ManualRustRootV1,
) -> Result<(RustLockstepAdapterV1, JavaLockstepResumeStateV1), String> {
    let origin = ManualSessionV1::load_existing(repository, &root.source_session_id)?;
    let (_, rust, java_resume) =
        replay_manual_session_prefix_v1(repository, &origin, root.source_committed_actions)?;
    Ok((rust, java_resume))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_run_control::eval::run_control::{
        RunControlSession, RunControlSessionCheckpointV1,
    };

    #[test]
    fn adopted_scope_order_keeps_each_branch_limit_and_rejects_cycles() {
        let repo =
            std::env::temp_dir().join(format!("manual-replay-chain-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&repo).unwrap();
        let first = ManualSessionV1::prepare(&repo).unwrap();
        let second = ManualSessionV1::prepare(&repo).unwrap();
        let third = ManualSessionV1::prepare(&repo).unwrap();
        let checkpoint = RunControlSessionCheckpointV1::from_session(&RunControlSession::new(
            Default::default(),
        ));
        second
            .publish_rust_root(first.session_id(), 7, checkpoint.clone())
            .unwrap();
        third
            .publish_rust_root(second.session_id(), 2, checkpoint.clone())
            .unwrap();
        assert_eq!(
            adopted_scopes(&repo, &third, None).unwrap(),
            vec![
                (first.clone(), Some(7)),
                (second.clone(), Some(2)),
                (third.clone(), None),
            ]
        );
        assert_eq!(
            adopted_scopes(&repo, &third, Some(1)).unwrap().last(),
            Some(&(third.clone(), Some(1)))
        );
        for anchor in [
            ("manual-unrelated", 1),
            (first.session_id(), 8),
            (third.session_id(), 2),
        ] {
            let mut succeeded = false;
            let result =
                replay_manual_scope_observed(&repo, &third, Some(1), Some(anchor), &mut |event| {
                    if let LockstepProgressV1::Finished {
                        succeeded: value, ..
                    } = event
                    {
                        succeeded = value;
                    }
                });
            assert!(result.is_err());
            assert!(
                !succeeded,
                "progress cannot promote an invalid source scope"
            );
        }
        first
            .publish_rust_root(third.session_id(), 1, checkpoint)
            .unwrap();
        assert!(adopted_scopes(&repo, &third, None)
            .unwrap_err()
            .contains("cyclic or too deep"));
        std::fs::remove_dir_all(repo).unwrap();
    }
}
