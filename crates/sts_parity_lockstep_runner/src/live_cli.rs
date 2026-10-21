//! JSON frontend for the existing manual runner. No game state or commit semantics here.
use super::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sts_parity_lockstep::{AdapterFailureKindV1, AdapterPhaseV1, EndpointV1, TransitionOutcomeV1};
use sts_parity_lockstep_runner::{describe_manual_actions_v1, DecisionSectionV1, DecisionStateV1};

#[derive(Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u64,
    #[serde(default)]
    view: Option<String>,
    operation: Operation,
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Observe,
    Inspect {
        #[serde(default)]
        section: DecisionSectionV1,
    },
    Choose {
        selector: Reference,
    },
    Adopt {
        proposal: String,
    },
    Close,
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Reference {
    Reference { reference: String },
}

fn validate(request: &Request, next_id: u64, scope: &str, stopped: bool) -> Result<(), String> {
    if request.id != next_id {
        return Err("Out-of-order request; use the next request ID".into());
    }
    if stopped
        && !matches!(
            request.operation,
            Operation::Observe | Operation::Inspect { .. } | Operation::Close
        )
    {
        return Err("Execution is stopped; inspect the failure before resuming".into());
    }
    if !matches!(request.operation, Operation::Observe | Operation::Close)
        && request.view.as_deref() != Some(scope)
    {
        return Err("Stale view; use the returned current choices".into());
    }
    Ok(())
}

pub(super) fn serve(
    runner: &mut ManualRunner,
    session: &ManualSessionV1,
    storage: &Path,
) -> Result<(), String> {
    capture_manual_checkpoints_on_attach(runner, session, storage)?;
    require_exact(runner)?;
    // Keep a last verified presentation so a failed transition cannot erase
    // its error receipt or mislabel a partial Rust state as Java + Rust.
    let mut current = DecisionStateV1::capture(
        runner.rust_adapter().session(),
        runner.current_snapshots().1,
    )?;
    let mut current_view = runner.current_action_view()?;
    let mut failure: Option<String> = None;
    let stdin = std::io::stdin();
    let mut output = std::io::stdout().lock();
    serve_protocol(stdin.lock(), &mut output, |request, next_id| {
        let start = std::time::Instant::now();
        let before = current.clone();
        let mut log = Vec::new();
        let mut closed = false;
        let result = validate(
            request,
            next_id,
            current_view.reference_prefix(),
            failure.is_some(),
        )
        .and_then(|()| {
            validate_choice(
                &request.operation,
                runner.rust_adapter().session().active_combat.is_some(),
            )?;
            let command = match &request.operation {
                Operation::Observe | Operation::Inspect { .. } => ManualCommandV1::Observe,
                Operation::Choose {
                    selector: Reference::Reference { reference },
                } => ManualCommandV1::Step(reference.clone()),
                Operation::Adopt { proposal } => ManualCommandV1::Adopt(proposal.into()),
                Operation::Close => ManualCommandV1::Park,
            };
            match execute_manual_command(runner, session, storage, command, &mut log) {
                Ok(receipt) => {
                    closed = receipt.closed;
                    receipt.rejection.map_or(Ok(()), Err)
                }
                Err(error) => {
                    failure = Some(error.clone());
                    Err(error)
                }
            }
        });
        if failure.is_none() {
            let refreshed = (|| {
                if runner.is_stopped() {
                    return Err(runner
                        .stop_reason()
                        .unwrap_or("Lockstep stopped")
                        .to_owned());
                }
                require_exact(runner)?;
                let state = DecisionStateV1::capture(
                    runner.rust_adapter().session(),
                    runner.current_snapshots().1,
                )?;
                let view = runner.current_action_view()?;
                Ok((state, view))
            })();
            match refreshed {
                Ok((state, view)) => {
                    current = state;
                    current_view = view;
                }
                Err(error) => failure = Some(error),
            }
        }
        let stopped = failure.is_some();
        let state = &current;
        let view = &current_view;
        let detail = matches!(request.operation, Operation::Inspect { .. });
        let section = match request.operation {
            Operation::Inspect { section } => section,
            _ => DecisionSectionV1::Position,
        };
        let visible_actions = section.choices(view.actions());
        let full = detail || request.operation == Operation::Observe || result.is_err() || stopped;
        let previous = if full { None } else { Some(&before) };
        let body = if let Some(body) = section
            .render(state)?
            .filter(|_| result.is_ok() && !stopped)
        {
            body
        } else if stopped || closed || state.combat.is_some() {
            state.render(previous, detail)?
        } else {
            state.render_with_action_menu(previous, detail)?
        };
        let mut labels = if stopped || state.combat.is_some() {
            Default::default()
        } else {
            describe_manual_actions_v1(
                runner.current_snapshots().1,
                &visible_actions,
                &runner.rust_adapter().event_action_labels_v1(),
            )?
        };
        if let Some(back) = &state.back {
            for action in &view.actions().actions {
                if action.action
                    == (SemanticActionV1::Cancel {
                        surface: DecisionSurfaceV1::Map,
                    })
                {
                    labels.insert(action.handle.clone(), back.clone());
                }
            }
        }
        let mut summary = format!(
            "{}: {}",
            if stopped {
                "Last verified position"
            } else {
                "Java + Rust"
            },
            if body.is_empty() {
                "Position unchanged"
            } else {
                &body
            }
        );
        if state.combat.is_none() && !closed && !stopped {
            let menu =
                sts_parity_lockstep_runner::render_decision_choices_v1(&visible_actions, &labels);
            if !menu.is_empty() {
                summary.push_str(&format!("\nChoices:\n{menu}"));
            } else if section.is_action_section() {
                summary.push_str("\nChoices: none");
            }
        }
        if let Some(error) = &failure {
            summary = format!(
                "Stopped: {}\n{summary}",
                stop_summary(error, runner.last_transition_outcome())
            );
        } else if let Err(error) = &result {
            summary.push_str(&format!("\nRejected: {error}"));
        }
        let reply = json!({
            "id":request.id, "workspace":session.session_id(), "view":view.reference_prefix(),
            "status": if stopped { "stopped" } else if result.is_err() { "rejected" } else { "ok" },
            "summary":summary, "error":failure.as_ref().or(result.as_ref().err()), "closed":closed,
            "choices":if stopped || closed || state.combat.is_some() { None } else { Some(&visible_actions) }, "choice_labels":labels,
            "state": if detail { Some(state) } else { None }, "state_is_current":!stopped,
            "execution": {"state": if stopped { "stopped" } else if closed { "parked" } else { "ready" },
                "parity": if stopped { "Stopped" } else { "Exact" }},
            "elapsed_micros": start.elapsed().as_micros() as u64,
            "diagnostic_log": String::from_utf8_lossy(&log),
            "transition_outcome": runner.last_transition_outcome(),
        });
        Ok(reply)
    })
}

fn stop_summary(error: &str, outcome: Option<&TransitionOutcomeV1>) -> String {
    if let Some(TransitionOutcomeV1::StoppedByAdapter {
        endpoint,
        phase,
        failure,
    }) = outcome
    {
        if failure.kind == AdapterFailureKindV1::Transport {
            let endpoint = match endpoint {
                EndpointV1::Java => "Java",
                EndpointV1::Rust => "Rust",
            };
            let status = match phase {
                AdapterPhaseV1::Prepare => "action preparation failed",
                AdapterPhaseV1::Execute | AdapterPhaseV1::ValidateSuccessor => {
                    "action outcome is unconfirmed"
                }
            };
            return format!("{endpoint} {status} (transport failure). Details: diagnostics().");
        }
    }
    error.to_owned()
}

fn validate_choice(operation: &Operation, combat: bool) -> Result<(), String> {
    if combat && matches!(operation, Operation::Choose { .. }) {
        return Err("Plan this combat in a Rust workspace, then adopt the verified victory".into());
    }
    Ok(())
}

// Wire lifetime and duplicate receipts only. The shared executor above owns
// every adoption, transition, persistence check and comparison.
fn serve_protocol(
    mut input: impl BufRead,
    output: &mut impl Write,
    mut handle: impl FnMut(&Request, u64) -> Result<Value, String>,
) -> Result<(), String> {
    let mut next_id = 1;
    let mut last: Option<(Request, Value)> = None;
    loop {
        let mut line = String::new();
        let bytes = {
            let _timing = sts_parity_lockstep::timing_span_v1("input.wait");
            input.read_line(&mut line).map_err(|e| e.to_string())?
        };
        if bytes == 0 {
            break;
        }
        let request: Request =
            serde_json::from_str(&line).map_err(|e| format!("Invalid session request: {e}"))?;
        let reply = match &last {
            Some((previous, reply)) if previous == &request => reply.clone(),
            _ => handle(&request, next_id)?,
        };
        let closed = reply["closed"].as_bool() == Some(true);
        if request.id == next_id {
            next_id += 1;
            last = Some((request, reply.clone()));
        }
        writeln!(output, "{reply}").map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        if closed {
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_stop_uses_typed_outcome_and_preserves_full_diagnostics() {
        let detail = r"timeout at D:\run with spaces\receipts\request.json";
        let outcome = TransitionOutcomeV1::StoppedByAdapter {
            endpoint: EndpointV1::Java,
            phase: AdapterPhaseV1::Execute,
            failure: sts_parity_lockstep::AdapterFailureV1::new(
                AdapterFailureKindV1::Transport,
                detail,
            ),
        };
        let summary = stop_summary(detail, Some(&outcome));
        assert!(summary.contains("Java action outcome is unconfirmed"));
        assert!(!summary.contains("D:"));
        assert_eq!(
            serde_json::to_value(&outcome).unwrap()["failure"]["message"],
            detail
        );
        // Missing typed evidence and actual comparison failures retain their reason.
        assert_eq!(stop_summary(detail, None), detail);
        let comparison = TransitionOutcomeV1::StoppedByComparison {
            reason: "RNG calls differ".into(),
        };
        assert_eq!(
            stop_summary("RNG calls differ", Some(&comparison)),
            "RNG calls differ"
        );
    }

    #[test]
    fn mutations_require_the_exact_view_and_order_and_cannot_continue_a_stop() {
        for operation in [
            Operation::Choose {
                selector: Reference::Reference {
                    reference: "V-source:1".into(),
                },
            },
            Operation::Adopt {
                proposal: "D-test".into(),
            },
        ] {
            let mut request = Request {
                id: 1,
                view: Some("V-source".into()),
                operation,
            };
            assert!(validate(&request, 1, "V-source", false).is_ok());
            assert!(validate(&request, 2, "V-source", false).is_err());
            assert!(validate(&request, 1, "V-new", false).is_err());
            assert!(validate(&request, 1, "V-source", true).is_err());
            request.view = None;
            assert!(validate(&request, 1, "V-source", false).is_err());
        }
    }

    #[test]
    fn duplicate_receipts_do_not_repeat_execution_and_conflicting_ids_are_rejected() {
        let choose = |id, reference| {
            json!({"id":id,"view":"V-source","operation":{
                "op":"choose","selector":{"kind":"reference","reference":reference}
            }})
            .to_string()
        };
        let first = choose(1, "V-source:0");
        let input = [
            first.clone(),
            first,
            choose(1, "V-source:1"),
            choose(2, "V-source:1"),
        ]
        .join("\n");
        let mut executions = 0;
        let mut output = Vec::new();
        serve_protocol(input.as_bytes(), &mut output, |request, next_id| {
            let accepted = validate(request, next_id, "V-source", false).is_ok();
            if accepted {
                executions += 1;
            }
            Ok(json!({"id":request.id,"accepted":accepted,"executions":executions}))
        })
        .unwrap();
        let replies: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(executions, 2);
        assert_eq!(replies[0], replies[1]);
        assert_eq!(replies[2]["accepted"], false);
        assert_eq!(replies[3]["executions"], 2);
    }

    #[test]
    fn combat_choices_cannot_bypass_victory_adoption() {
        let choose = Operation::Choose {
            selector: Reference::Reference {
                reference: "V-source:0".into(),
            },
        };
        assert!(validate_choice(&choose, false).is_ok());
        assert!(validate_choice(&choose, true).is_err());
        assert!(validate_choice(
            &Operation::Adopt {
                proposal: "D-test".into()
            },
            true
        )
        .is_ok());
        // Permission to request adoption is not verification; the executor
        // still calls prepare_manual_draft_adoption_v1 before any Java action.
    }
}
