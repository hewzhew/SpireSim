use std::collections::BTreeMap;

use super::super::rules::{candidate_level, latent_debt_kind};
use super::super::types::*;

pub(super) fn identity_audit_report(
    cases: &[StateAbstractionCaseReport],
) -> StateAbstractionIdentityAuditReport {
    let mut candidate_cases = BTreeMap::<&str, usize>::new();
    let mut candidate_groups = 0usize;
    let mut samples = Vec::new();

    for case in cases {
        for entry in &case.turn_sequence_divergence_histogram {
            let debt = latent_debt_kind(entry.kind);
            let level = candidate_level(
                entry.kind,
                debt,
                entry.first_divergence_path,
                entry.guessed_reveal_gate,
            );
            if level != StateAbstractionCandidateLevel::IdentityAuditCandidate {
                continue;
            }

            candidate_groups = candidate_groups.saturating_add(entry.groups);
            *candidate_cases.entry(&case.case_id).or_default() += entry.groups;
            if samples.len() < 8 {
                samples.push(StateAbstractionIdentityAuditSample {
                    case_id: case.case_id.clone(),
                    groups: entry.groups,
                    first_divergence_path: entry.first_divergence_path,
                    guessed_reveal_gate: entry.guessed_reveal_gate,
                    required_next_check: "prove_lineage_not_referenced_by_pending_queue_legal_actions_or_discard_selection_before_shuffle",
                });
            }
        }
    }

    StateAbstractionIdentityAuditReport {
        audit_policy: "card_lineage_identity_candidates_are_blocked_until_reference_audit",
        behavioral_effect: "report_only_no_prune_no_state_merge",
        status: if candidate_groups == 0 {
            "no_identity_audit_candidates_observed"
        } else {
            "blocked_until_card_identity_reference_audit"
        },
        candidate_cases: candidate_cases.len(),
        candidate_groups,
        proof_pruning_enabled: false,
        exact_branch_removal_allowed: false,
        blocked_reason: "card lineage differences can be referenced by pending queues, legal action descriptors, selection payloads, or future card identity lookups",
        required_checks: vec![
            "pending queue and queued card payloads do not reference the differing lineage",
            "current legal action descriptors and selectable payloads are unchanged by the lineage order",
            "no discard-pile card selection can read the differing lineage before the reveal gate",
            "shuffle or other reveal-gate handling must split back to exact representatives before using the abstraction",
        ],
        samples,
        notes: vec![
            "identity_audit_candidate is deliberately blocked from prune-safe consumers in v1",
            "discard pile lineage-order differences may look similar to discard-order debt but are a separate identity-reference problem",
            "this report is an audit target list, not a search optimization",
        ],
    }
}
