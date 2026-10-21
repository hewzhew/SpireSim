use crate::domain_manifest::manifest_scalar_v1;
use crate::search_lease::ResolvedWitnessAliasV1;
use std::fmt::Write;
use std::path::Path;
use sts_workbench_engine_protocol::{
    EngineWitnessPreviewCompletionV1, EngineWitnessPreviewStatusV1,
    EngineWitnessPreviewStepStatusV1,
};

const WITNESS_PREVIEW_MANIFEST_VERSION_V1: &str = "STS-WITNESS-PREVIEW 1";

pub(super) struct RenderedWitnessPreviewV1 {
    pub text: String,
    pub detail_handle: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WitnessPreviewRenderErrorV1 {
    pub code: &'static str,
    pub detail: String,
}

pub(super) fn render_witness_preview_v1(
    repository: &Path,
    resolved: &ResolvedWitnessAliasV1,
    completion: &EngineWitnessPreviewCompletionV1,
) -> Result<RenderedWitnessPreviewV1, WitnessPreviewRenderErrorV1> {
    resolved
        .scope
        .validate(repository)
        .map_err(|error| render_error("WitnessExpired", error.detail))?;
    if completion.root_id != resolved.scope.root_id
        || completion.action_space_id != resolved.scope.action_space_id
        || completion.witness_handle != resolved.handle
        || !paired_root_id_has_shape(&completion.reached_root_id)
        || !artifact_handle_has_shape(&completion.detail_handle)
        || completion.steps.len() > usize::try_from(resolved.plan_step_count).unwrap_or(usize::MAX)
    {
        return Err(render_error(
            "WitnessPreviewInvalid",
            "preview completion is not bound to its W alias",
        ));
    }

    let mut step_lines = Vec::with_capacity(completion.steps.len().saturating_add(1));
    for (ordinal, step) in completion.steps.iter().enumerate() {
        let valid = match step.status {
            EngineWitnessPreviewStepStatusV1::Ready => {
                step.total_count > 0 && step.exact_count == step.total_count
            }
            EngineWitnessPreviewStepStatusV1::Diverged => {
                step.total_count > 0 && step.exact_count < step.total_count
            }
            EngineWitnessPreviewStepStatusV1::SuccessorBoundaryUnsupported => {
                step.exact_count == 0 && step.total_count == 0
            }
        };
        if !valid
            || step.status != EngineWitnessPreviewStepStatusV1::Ready
                && ordinal + 1 != completion.steps.len()
            || step.status != EngineWitnessPreviewStepStatusV1::Ready
                && completion.stopped_step_ordinal != u64::try_from(ordinal).ok()
        {
            return Err(render_error(
                "WitnessPreviewInvalid",
                "steps have an invalid terminal boundary",
            ));
        }
        let line = if step.status == EngineWitnessPreviewStepStatusV1::Ready {
            format!("S{ordinal} exact={}/{}", step.exact_count, step.total_count)
        } else {
            format!(
                "S{ordinal} stop={} exact={}/{}",
                step.status.token(),
                step.exact_count,
                step.total_count
            )
        };
        step_lines.push(line);
    }

    match completion.status {
        EngineWitnessPreviewStatusV1::Ready
            if completion.stopped_step_ordinal.is_none()
                && completion.stop_reason.is_none()
                && completion.steps.len()
                    == usize::try_from(resolved.plan_step_count).unwrap_or(usize::MAX)
                && completion
                    .steps
                    .iter()
                    .all(|step| step.status == EngineWitnessPreviewStepStatusV1::Ready) => {}
        EngineWitnessPreviewStatusV1::IntentUnavailable
            if completion.stopped_step_ordinal == u64::try_from(completion.steps.len()).ok()
                && completion
                    .stop_reason
                    .as_deref()
                    .is_some_and(|reason| !reason.trim().is_empty()) =>
        {
            step_lines.push(format!(
                "S{} stop=intent_unavailable",
                completion.steps.len()
            ));
        }
        EngineWitnessPreviewStatusV1::Diverged
        | EngineWitnessPreviewStatusV1::SuccessorBoundaryUnsupported
            if completion.stopped_step_ordinal
                == u64::try_from(completion.steps.len().saturating_sub(1)).ok()
                && completion
                    .stop_reason
                    .as_deref()
                    .is_some_and(|reason| !reason.trim().is_empty()) => {}
        _ => {
            return Err(render_error(
                "WitnessPreviewInvalid",
                "status disagrees with its stopped step",
            ));
        }
    }

    let completed = completion
        .steps
        .iter()
        .filter(|step| step.status == EngineWitnessPreviewStepStatusV1::Ready)
        .count();
    let authority = match &resolved.scope.source {
        crate::lease_source::LeaseSourceV1::ExplicitRoot => "observe_only",
        crate::lease_source::LeaseSourceV1::SelectedHead(_) => "lab_head",
    };
    let mut text = format!(
        "{WITNESS_PREVIEW_MANIFEST_VERSION_V1}\nauthority={authority}\n\n[witness]\n{} status={} exact_steps={completed}/{} continue={} commit=unavailable commit_reason=sequential_receipts_not_atomic inspect=D0",
        resolved.alias,
        completion.status.token(),
        resolved.plan_step_count,
        if completion.status == EngineWitnessPreviewStatusV1::Ready {
            "P0"
        } else {
            "unavailable"
        },
    );
    if let Some(step) = completion.stopped_step_ordinal {
        write!(text, " stopped=S{step}").unwrap();
    }
    if let Some(reason) = &completion.stop_reason {
        write!(
            text,
            " stop_reason={}",
            manifest_scalar_v1(reason)
                .map_err(|detail| render_error("WitnessPreviewInvalid", detail))?
        )
        .unwrap();
    }
    writeln!(text, "\n\n[steps]").unwrap();
    for line in step_lines {
        writeln!(text, "{line}").unwrap();
    }
    Ok(RenderedWitnessPreviewV1 {
        text,
        detail_handle: completion.detail_handle.clone(),
    })
}

fn render_error(code: &'static str, detail: impl Into<String>) -> WitnessPreviewRenderErrorV1 {
    WitnessPreviewRenderErrorV1 {
        code,
        detail: detail.into(),
    }
}

fn artifact_handle_has_shape(value: &str) -> bool {
    value
        .strip_prefix("workbench-artifact-v1-")
        .is_some_and(|digest| {
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn paired_root_id_has_shape(value: &str) -> bool {
    value.strip_prefix("state-root-v2-").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lease_source::{LeaseScopeV1, LeaseSourceV1};
    use crate::test_support::{add_fixture_successor, paired_store_fixture};
    use sts_paired_root::PairedActionV1;
    use sts_workbench_engine_protocol::{
        EngineWitnessPreviewStepV1, ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1,
    };

    fn artifact(byte: char) -> String {
        format!(
            "workbench-artifact-v1-{}",
            std::iter::repeat_n(byte, 64).collect::<String>()
        )
    }

    fn resolved(
        fixture: &crate::test_support::PairedStoreFixture,
        handle: &str,
        steps: u64,
    ) -> ResolvedWitnessAliasV1 {
        ResolvedWitnessAliasV1 {
            alias: "W0".to_owned(),
            handle: handle.to_owned(),
            plan_step_count: steps,
            scope: LeaseScopeV1::new(
                fixture.source.root_id.clone(),
                fixture.source.root.preflight.action_space_id.clone(),
                LeaseSourceV1::ExplicitRoot,
            ),
        }
    }

    #[test]
    fn ready_witness_preview_is_stepwise_short_and_hash_free() {
        let fixture = paired_store_fixture();
        let first_root = fixture
            .store
            .load_root(&fixture.first.receipt.successor_root_id)
            .unwrap();
        let second_step =
            add_fixture_successor(&fixture.store, &first_root, PairedActionV1::EndTurn);
        let handle = artifact('a');
        let resolved = resolved(&fixture, &handle, 2);
        let completion = EngineWitnessPreviewCompletionV1 {
            schema: ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1.to_owned(),
            root_id: fixture.source.root_id.clone(),
            action_space_id: fixture.source.root.preflight.action_space_id.clone(),
            witness_handle: handle,
            reached_root_id: second_step.receipt.successor_root_id,
            status: EngineWitnessPreviewStatusV1::Ready,
            steps: vec![
                EngineWitnessPreviewStepV1 {
                    status: EngineWitnessPreviewStepStatusV1::Ready,
                    exact_count: 9,
                    total_count: 9,
                },
                EngineWitnessPreviewStepV1 {
                    status: EngineWitnessPreviewStepStatusV1::Ready,
                    exact_count: 11,
                    total_count: 11,
                },
            ],
            stopped_step_ordinal: None,
            stop_reason: None,
            detail_handle: artifact('b'),
        };
        let rendered =
            render_witness_preview_v1(&fixture.repository, &resolved, &completion).unwrap();
        assert!(rendered.text.contains("W0 status=ready exact_steps=2/2"));
        assert!(rendered.text.contains("continue=P0"));
        assert!(rendered.text.contains("S0 exact=9/9\nS1 exact=11/11"));
        assert!(rendered.text.contains("commit=unavailable"));
        assert!(!rendered.text.contains("workbench-artifact"));
        assert!(!rendered.text.contains("state-root-v2"));
    }

    #[test]
    fn unavailable_intent_has_one_explicit_stop_line() {
        let fixture = paired_store_fixture();
        let handle = artifact('a');
        let resolved = resolved(&fixture, &handle, 3);
        let completion = EngineWitnessPreviewCompletionV1 {
            schema: ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1.to_owned(),
            root_id: fixture.source.root_id.clone(),
            action_space_id: fixture.source.root.preflight.action_space_id.clone(),
            witness_handle: handle,
            reached_root_id: fixture.first.receipt.successor_root_id.clone(),
            status: EngineWitnessPreviewStatusV1::IntentUnavailable,
            steps: vec![EngineWitnessPreviewStepV1 {
                status: EngineWitnessPreviewStepStatusV1::Ready,
                exact_count: 9,
                total_count: 9,
            }],
            stopped_step_ordinal: Some(1),
            stop_reason: Some("typed action unavailable".to_owned()),
            detail_handle: artifact('b'),
        };
        let rendered =
            render_witness_preview_v1(&fixture.repository, &resolved, &completion).unwrap();
        assert!(rendered.text.contains("stopped=S1"));
        assert!(rendered.text.contains("S1 stop=intent_unavailable"));
    }
}
