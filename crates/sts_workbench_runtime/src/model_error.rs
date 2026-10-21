struct ModelErrorGuidanceV1 {
    effect: &'static str,
    retry: &'static str,
    message: &'static str,
    next: &'static str,
}

pub(super) fn render(code: &str) -> String {
    let public_code = if valid_code(code) {
        code
    } else {
        "runtime_rejected"
    };
    let guidance = guidance(public_code);
    format!(
        "STS-ERROR 1\ncode={public_code}\neffect={}\nretry={}\nmessage {}\nnext {}\n",
        guidance.effect, guidance.retry, guidance.message, guidance.next
    )
}

fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn guidance(code: &str) -> ModelErrorGuidanceV1 {
    match code {
        "runtime_prerequisite_stale" => ModelErrorGuidanceV1 {
            effect: "request_not_started",
            retry: "after_build",
            message: "The workbench runtime closure is stale.",
            next: r"Run .\workbench.ps1 build, then use service_status only to check whether the actor must restart.",
        },
        "ObserveRequired" => ModelErrorGuidanceV1 {
            effect: "none",
            retry: "after_observe",
            message: "There is no current action view.",
            next: "Call observe before using an A alias.",
        },
        "ActionUnknown" => ModelErrorGuidanceV1 {
            effect: "none",
            retry: "after_observe",
            message: "That action alias is not present in the current view.",
            next: "Call observe and choose an alias from its actions section.",
        },
        "ActionExpired" => ModelErrorGuidanceV1 {
            effect: "none",
            retry: "after_reobserve",
            message: "That action alias belongs to an expired view.",
            next: "Call observe again before previewing an action.",
        },
        "ActionUnavailable" => ModelErrorGuidanceV1 {
            effect: "none",
            retry: "forbidden",
            message: "That observed action is not authorized for preview.",
            next: "Keep the current root unchanged and inspect its reported coverage gap.",
        },
        "PreviewRequired" => ModelErrorGuidanceV1 {
            effect: "none",
            retry: "after_preview",
            message: "There is no current preview receipt.",
            next: "Preview an action before using a P alias.",
        },
        "CurrentManifestUnavailable" => ModelErrorGuidanceV1 {
            effect: "none",
            retry: "forbidden",
            message: "The selected root is historical and has no current manifest surface.",
            next: "Observe an explicit current root or create a current natural root before changing the selected head.",
        },
        "view_execution_in_progress" => ModelErrorGuidanceV1 {
            effect: "official_head_unchanged",
            retry: "forbidden",
            message: "The Engine still owns an unfinished preview transaction.",
            next: "Do not retry automatically; treat a repeat as a lifecycle defect, not a health-check problem.",
        },
        "engine_job_in_progress" => ModelErrorGuidanceV1 {
            effect: "durable_state_unchanged",
            retry: "forbidden",
            message: "The Engine is still completing an earlier supervised job.",
            next: "Let that job settle or retire before issuing another operation.",
        },
        "runtime_state_inconsistent" => ModelErrorGuidanceV1 {
            effect: "durable_state_unchanged",
            retry: "forbidden",
            message: "The Java owner and Engine view no longer prove the same stable boundary.",
            next: "Do not continue from this runtime; recover from durable source evidence.",
        },
        "parked_runtime_validation_failed" => ModelErrorGuidanceV1 {
            effect: "durable_state_unchanged",
            retry: "forbidden",
            message: "The parked Java runtime no longer validates against its accepted boundary.",
            next: "Do not reuse that runtime; recover the source root in a clean runtime.",
        },
        "engine_unavailable" => ModelErrorGuidanceV1 {
            effect: "request_not_started",
            retry: "after_build",
            message: "The replaceable Engine is unavailable or stale.",
            next: r"Run .\workbench.ps1 build; an App restart is not required for an Engine-only update.",
        },
        "engine_operation_failed" => ModelErrorGuidanceV1 {
            effect: "unconfirmed",
            retry: "forbidden",
            message: "The supervised Engine operation failed before producing a trusted result.",
            next: "Do not retry automatically; use the locally stored rejection evidence for development.",
        },
        "engine_request_invalid" => ModelErrorGuidanceV1 {
            effect: "request_not_started",
            retry: "forbidden",
            message: "The runtime could not form the closed Engine request.",
            next: "Fix the runtime-to-Engine contract and rebuild before retrying.",
        },
        "engine_response_invalid" => ModelErrorGuidanceV1 {
            effect: "unconfirmed",
            retry: "forbidden",
            message: "The Engine response did not satisfy the closed runtime contract.",
            next: "Do not continue from the attempted successor; fix the contract and rebuild.",
        },
        "preview_action_aliases_failed" => ModelErrorGuidanceV1 {
            effect: "official_head_unchanged",
            retry: "forbidden",
            message: "The action preview stopped before a trusted successor was accepted.",
            next: "Do not retry automatically; use the locally stored rejection evidence for development.",
        },
        "persistent_master_deck_diverged" => ModelErrorGuidanceV1 {
            effect: "official_head_unchanged",
            retry: "forbidden",
            message: "Java and Rust produced different persistent master decks.",
            next: "Do not retry automatically; compare the locally stored deck semantics and fix the first divergent transition.",
        },
        "paired_root_version_drift" => ModelErrorGuidanceV1 {
            effect: "request_not_started",
            retry: "forbidden",
            message: "The selected root belongs to a different Java or simulator build.",
            next: "Start a fresh current-generation natural run; sealed history is inspect-only and must not be replayed or migrated.",
        },
        "run_play_adapter_failed" | "run_play_adapter_protocol_invalid" => {
            ModelErrorGuidanceV1 {
                effect: "unconfirmed",
                retry: "forbidden",
                message: "The typed run-play adapter stopped without a trusted result.",
                next: "Do not retry automatically; use the locally stored adapter failure evidence for development.",
            }
        }
        "preview_witness_alias_failed" => ModelErrorGuidanceV1 {
            effect: "official_head_unchanged",
            retry: "forbidden",
            message: "The witness preview stopped before a trusted successor chain was accepted.",
            next: "Do not retry automatically; use the locally stored rejection evidence for development.",
        },
        _ => ModelErrorGuidanceV1 {
            effect: "unconfirmed",
            retry: "forbidden",
            message: "The workbench rejected this operation with a closed cause code.",
            next: "Do not retry automatically; use the locally stored rejection evidence for development.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_invalid_codes_have_a_closed_fallback() {
        let future = render("future_closed_failure");
        assert!(future.contains("code=future_closed_failure"));
        assert!(future.contains("effect=unconfirmed"));
        assert!(future.contains("retry=forbidden"));

        let invalid = render("bad code\npath=D:/private");
        assert!(invalid.contains("code=runtime_rejected"));
        assert!(!invalid.contains("D:/private"));
    }

    #[test]
    fn persistent_deck_divergence_is_short_and_forbids_retry() {
        let rendered = render("persistent_master_deck_diverged");

        assert!(rendered.contains("code=persistent_master_deck_diverged"));
        assert!(rendered.contains("effect=official_head_unchanged"));
        assert!(rendered.contains("retry=forbidden"));
        assert!(!rendered.contains("private"));
    }

    #[test]
    fn version_drift_explains_that_execution_did_not_start() {
        let rendered = render("paired_root_version_drift");

        assert!(rendered.contains("code=paired_root_version_drift"));
        assert!(rendered.contains("effect=request_not_started"));
        assert!(rendered.contains("retry=forbidden"));
        assert!(rendered.contains("Start a fresh current-generation natural run"));
        assert!(rendered.contains("must not be replayed or migrated"));
    }
}
