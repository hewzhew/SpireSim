//! Conservative offline policy improvement from typed action reanalysis.
//!
//! Exact wins receive an explicit bounded support mass. Budget-unknown actions
//! retain their relative base-policy mass and are never relabeled as losses.
//! Exact refutations and terminal non-wins are the only zero-target actions.

use std::path::PathBuf;

use clap::Args;
use serde::Deserialize;
use serde_json::{json, Value};
use sts_combat_planner::{CombatActionPolicy, CombatPolicyChoice};
use sts_oracle_runtime::eval::combat_action_imitation::{
    audit_combat_action_imitation_v1, combat_action_imitation_policy_v1,
    combat_action_reanalysis_has_v1_preference_evidence, conservative_combat_reanalysis_target_v1,
    train_combat_action_imitation_with_reanalysis_and_base_v1,
    CombatActionImitationDemonstrationV1, CombatActionImitationTrainingConfigV1,
    CombatActionReanalysisCandidateV1, CombatActionReanalysisDecisionV1,
    CombatActionReanalysisEvidenceV1, CombatActionReanalysisTrainingConfigV1,
};
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::CombatPosition;
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_trace_view::combat_action_label;
use super::exact_combat_evidence::ExactCombatEvidence;
use super::exact_turn_corridor::load_corpus as load_combat_action_imitation_corpus;

const CORPUS_SCHEMA: &str = "ActionSuccessorReanalysisCorpusV2";

#[derive(Debug, Args)]
pub(crate) struct ActionReanalysisPolicyArgs {
    /// Existing exact-witness training manifest.
    #[arg(long)]
    manifest: PathBuf,
    /// One or more typed action-successor evidence corpora.
    #[arg(long, required = true)]
    reanalysis_corpus: Vec<PathBuf>,
    /// Destination for the combined residual policy artifact.
    #[arg(long)]
    output: PathBuf,
    /// Explicit mass transferred to exact-win support at each reanalysed state.
    #[arg(long, default_value_t = 0.5)]
    exact_support_mass: f64,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

#[derive(Debug, Deserialize)]
struct ReanalysisCorpus {
    schema_name: String,
    schema_version: u32,
    root_position: CombatPosition,
    surface: ReanalysisSurface,
    candidates: Vec<ReanalysisCandidate>,
}

#[derive(Debug, Deserialize)]
struct ReanalysisSurface {
    complete: bool,
    materialized_candidates: usize,
    atomic_actions: usize,
    structured_family_count: usize,
}

#[derive(Debug, Deserialize)]
struct ReanalysisCandidate {
    input: ClientInput,
    label: String,
    evidence: ExactCombatEvidence,
}

struct LoadedReanalysisDecision {
    source: PathBuf,
    root: CombatPosition,
    candidates: Vec<CombatActionReanalysisCandidateV1>,
    labels: Vec<String>,
}

struct ReanalysisAudit {
    report: Value,
    source: PathBuf,
    contains_budget_unknown: bool,
    contains_exact_non_win: bool,
    base_exact_win_mass: f64,
    learned_exact_win_mass: f64,
    target_exact_win_mass: f64,
    base_target_total_variation: f64,
    learned_target_total_variation: f64,
}

pub(crate) fn build(args: ActionReanalysisPolicyArgs) -> Result<Value, String> {
    if args.max_engine_steps_per_transition == 0 {
        return Err("action reanalysis policy transition budget must be positive".to_string());
    }
    let reanalysis_config = CombatActionReanalysisTrainingConfigV1 {
        exact_support_mass: args.exact_support_mass,
    };
    let demonstrations = load_combat_action_imitation_corpus(&args.manifest)?;
    let loaded_reanalysis = args
        .reanalysis_corpus
        .iter()
        .map(|path| load_reanalysis_corpus(path))
        .collect::<Result<Vec<_>, String>>()?;

    let borrowed_demonstrations = demonstrations
        .iter()
        .map(|demonstration| CombatActionImitationDemonstrationV1 {
            root: &demonstration.position,
            actions: &demonstration.actions,
        })
        .collect::<Vec<_>>();
    let borrowed_reanalysis = loaded_reanalysis
        .iter()
        .map(|decision| CombatActionReanalysisDecisionV1 {
            root: &decision.root,
            candidates: &decision.candidates,
        })
        .collect::<Vec<_>>();
    let training_config = CombatActionImitationTrainingConfigV1 {
        max_engine_steps_per_transition: args.max_engine_steps_per_transition,
        base_weight_exponent: 1.0,
        ..CombatActionImitationTrainingConfigV1::default()
    };
    let base_policy = retained_combat_tactical_policy_v1();
    let artifact = train_combat_action_imitation_with_reanalysis_and_base_v1(
        &borrowed_demonstrations,
        &borrowed_reanalysis,
        training_config,
        reanalysis_config,
        base_policy.clone(),
    )?;
    let learned_policy = combat_action_imitation_policy_v1(base_policy.clone(), artifact.clone())?;

    let demonstration_audits = demonstrations
        .iter()
        .map(|demonstration| {
            audit_combat_action_imitation_v1(
                &demonstration.position,
                &demonstration.actions,
                &artifact,
                base_policy.as_ref(),
                training_config.max_structured_alternatives,
                args.max_engine_steps_per_transition,
            )
            .map(|audit| {
                json!({
                    "id": demonstration.id,
                    "source_action_count": audit.source_action_count,
                    "ranked_decision_count": audit.ranked_decision_count,
                    "skipped_forced_decision_count": audit.skipped_forced_decision_count,
                    "miss_count": audit.misses.len(),
                })
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let reanalysis_audits = loaded_reanalysis
        .iter()
        .map(|decision| {
            audit_reanalysis_decision(
                decision,
                base_policy.as_ref(),
                learned_policy.as_ref(),
                reanalysis_config,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let v1_informative_reanalysis_decision_count = reanalysis_audits
        .iter()
        .filter(|audit| audit.contains_budget_unknown || audit.contains_exact_non_win)
        .count();
    let promotion_failures = reanalysis_audits
        .iter()
        .filter_map(|audit| {
            let reason = promotion_gate_failure_reason(audit)?;
            Some(json!({
                "source": audit.source,
                "reason": reason,
                "base_exact_win_mass": audit.base_exact_win_mass,
                "learned_exact_win_mass": audit.learned_exact_win_mass,
                "target_exact_win_mass": audit.target_exact_win_mass,
                "base_target_total_variation": audit.base_target_total_variation,
                "learned_target_total_variation": audit.learned_target_total_variation,
            }))
        })
        .collect::<Vec<_>>();
    let artifact_saved = promotion_failures.is_empty();
    if artifact_saved {
        artifact.save(&args.output)?;
    } else if args.output.exists() {
        std::fs::remove_file(&args.output).map_err(|error| {
            format!(
                "failed to remove rejected action policy artifact {}: {error}",
                args.output.display()
            )
        })?;
    }
    Ok(json!({
        "schema_name": "OracleCombatActionReanalysisPolicyBuildV1",
        "schema_version": 1,
        "manifest": args.manifest,
        "reanalysis_corpora": args.reanalysis_corpus,
        "output": args.output,
        "training_base": "existing_combat_knowledge_v1",
        "exact_support_mass": args.exact_support_mass,
        "promotion": {
            "status": if artifact_saved { "accepted" } else { "rejected" },
            "artifact_saved": artifact_saved,
            "contract": "v1_informative_reanalysis_only_closer_to_target_and_no_exact_win_mass_regression_under_uncertainty",
            "failures": promotion_failures,
        },
        "artifact": {
            "schema_name": artifact.schema_name,
            "schema_version": artifact.schema_version,
            "feature_schema": artifact.feature_schema,
            "runtime_compatibility_id": artifact.runtime_compatibility_id,
            "training_authority": artifact.training_authority,
            "source_trajectory_count": artifact.source_trajectory_count,
            "source_action_count": artifact.source_action_count,
            "demonstration_trajectory_count": demonstrations.len(),
            "reanalysis_decision_count": loaded_reanalysis.len(),
            "v1_informative_reanalysis_decision_count": v1_informative_reanalysis_decision_count,
            "ranked_decision_count": artifact.ranked_decision_count,
            "pairwise_comparison_count": artifact.pairwise_comparison_count,
            "training_top1_correct": artifact.training_top1_correct,
            "training_top1_total": artifact.training_top1_total,
            "coefficient_count": artifact.coefficients.len(),
        },
        "demonstrations": demonstration_audits,
        "reanalysis": reanalysis_audits
            .into_iter()
            .map(|audit| audit.report)
            .collect::<Vec<_>>(),
    }))
}

fn promotion_gate_failure_reason(audit: &ReanalysisAudit) -> Option<&'static str> {
    let has_preference_evidence = audit.contains_budget_unknown || audit.contains_exact_non_win;
    if has_preference_evidence
        && audit.learned_target_total_variation > audit.base_target_total_variation + 1.0e-12
    {
        return Some("learned_policy_farther_from_typed_reanalysis_target_than_base");
    }
    if audit.contains_budget_unknown
        && audit.learned_exact_win_mass + 1.0e-12 < audit.base_exact_win_mass
    {
        return Some("learned_exact_win_mass_below_base_with_budget_unknown");
    }
    None
}

fn load_reanalysis_corpus(path: &PathBuf) -> Result<LoadedReanalysisDecision, String> {
    let corpus = serde_json::from_slice::<ReanalysisCorpus>(
        &std::fs::read(path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "invalid action reanalysis corpus {}: {error}",
            path.display()
        )
    })?;
    if corpus.schema_name != CORPUS_SCHEMA || corpus.schema_version != 2 {
        return Err(format!(
            "unsupported action reanalysis corpus schema in {}",
            path.display()
        ));
    }
    if !corpus.surface.complete
        || corpus.surface.structured_family_count != 0
        || corpus.surface.materialized_candidates != corpus.candidates.len()
        || corpus.surface.atomic_actions != corpus.candidates.len()
    {
        return Err(format!(
            "action reanalysis corpus {} does not contain a complete atomic surface",
            path.display()
        ));
    }
    let mut candidates = Vec::with_capacity(corpus.candidates.len());
    let mut labels = Vec::with_capacity(corpus.candidates.len());
    for candidate in corpus.candidates {
        labels.push(candidate.label);
        candidates.push(CombatActionReanalysisCandidateV1 {
            input: candidate.input,
            evidence: match candidate.evidence {
                ExactCombatEvidence::ExactWin { final_hp, .. } => {
                    CombatActionReanalysisEvidenceV1::ExactWin { final_hp }
                }
                ExactCombatEvidence::ExactRefutation { .. }
                | ExactCombatEvidence::ExactTerminalNonWin { .. } => {
                    CombatActionReanalysisEvidenceV1::ExactNonWin
                }
                ExactCombatEvidence::BudgetUnknown { .. } => {
                    CombatActionReanalysisEvidenceV1::BudgetUnknown
                }
            },
        });
    }
    Ok(LoadedReanalysisDecision {
        source: path.clone(),
        root: corpus.root_position,
        candidates,
        labels,
    })
}

fn audit_reanalysis_decision(
    decision: &LoadedReanalysisDecision,
    base_policy: &dyn CombatActionPolicy,
    learned_policy: &dyn CombatActionPolicy,
    config: CombatActionReanalysisTrainingConfigV1,
) -> Result<ReanalysisAudit, String> {
    let choices = decision
        .candidates
        .iter()
        .map(|candidate| CombatPolicyChoice::Atomic(&candidate.input))
        .collect::<Vec<_>>();
    let base_weights = base_policy.weights(&decision.root, &choices);
    let learned_weights = learned_policy.weights(&decision.root, &choices);
    if base_weights.len() != decision.candidates.len()
        || learned_weights.len() != decision.candidates.len()
    {
        return Err(format!(
            "action reanalysis policy audit received misaligned weights for {}",
            decision.source.display()
        ));
    }
    let evidence = decision
        .candidates
        .iter()
        .map(|candidate| candidate.evidence)
        .collect::<Vec<_>>();
    let has_v1_preference_evidence = combat_action_reanalysis_has_v1_preference_evidence(&evidence);
    let target_probabilities =
        conservative_combat_reanalysis_target_v1(&base_weights, &evidence, config)?;
    let base_probabilities = normalized_probabilities(&base_weights);
    let learned_probabilities = normalized_probabilities(&learned_weights);
    let base_ranks = ranks(&base_weights);
    let learned_ranks = ranks(&learned_weights);
    let exact_win_mass = |probabilities: &[f64]| {
        probabilities
            .iter()
            .zip(&evidence)
            .filter(|(_, evidence)| {
                matches!(evidence, CombatActionReanalysisEvidenceV1::ExactWin { .. })
            })
            .map(|(probability, _)| *probability)
            .sum::<f64>()
    };
    let budget_unknown_mass = |probabilities: &[f64]| {
        probabilities
            .iter()
            .zip(&evidence)
            .filter(|(_, evidence)| {
                matches!(evidence, CombatActionReanalysisEvidenceV1::BudgetUnknown)
            })
            .map(|(probability, _)| *probability)
            .sum::<f64>()
    };
    let exact_non_win_mass = |probabilities: &[f64]| {
        probabilities
            .iter()
            .zip(&evidence)
            .filter(|(_, evidence)| {
                matches!(evidence, CombatActionReanalysisEvidenceV1::ExactNonWin)
            })
            .map(|(probability, _)| *probability)
            .sum::<f64>()
    };
    let total_variation = |left: &[f64], right: &[f64]| {
        0.5 * left
            .iter()
            .zip(right)
            .map(|(left, right)| (left - right).abs())
            .sum::<f64>()
    };
    let base_target_total_variation = total_variation(&base_probabilities, &target_probabilities);
    let learned_target_total_variation =
        total_variation(&learned_probabilities, &target_probabilities);
    let candidates = decision
        .candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            json!({
                "label": decision.labels.get(index).cloned().unwrap_or_else(|| {
                    combat_action_label(&decision.root, &candidate.input)
                }),
                "input": candidate.input,
                "evidence": evidence_name(candidate.evidence),
                "base_rank": base_ranks[index],
                "learned_rank": learned_ranks[index],
                "base_weight": base_weights[index],
                "learned_weight": learned_weights[index],
                "base_probability": base_probabilities[index],
                "learned_probability": learned_probabilities[index],
                "target_probability": target_probabilities[index],
            })
        })
        .collect::<Vec<_>>();
    let base_exact_win_mass = exact_win_mass(&base_probabilities);
    let learned_exact_win_mass = exact_win_mass(&learned_probabilities);
    let target_exact_win_mass = exact_win_mass(&target_probabilities);
    let contains_budget_unknown = evidence
        .iter()
        .any(|evidence| matches!(evidence, CombatActionReanalysisEvidenceV1::BudgetUnknown));
    let contains_exact_non_win = evidence
        .iter()
        .any(|evidence| matches!(evidence, CombatActionReanalysisEvidenceV1::ExactNonWin));
    let report = json!({
        "source": decision.source,
        "candidate_count": decision.candidates.len(),
        "has_v1_preference_evidence": has_v1_preference_evidence,
        "mass_by_evidence": {
            "base": {
                "exact_win": base_exact_win_mass,
                "budget_unknown": budget_unknown_mass(&base_probabilities),
                "exact_non_win": exact_non_win_mass(&base_probabilities),
            },
            "target": {
                "exact_win": target_exact_win_mass,
                "budget_unknown": budget_unknown_mass(&target_probabilities),
                "exact_non_win": exact_non_win_mass(&target_probabilities),
            },
            "learned": {
                "exact_win": learned_exact_win_mass,
                "budget_unknown": budget_unknown_mass(&learned_probabilities),
                "exact_non_win": exact_non_win_mass(&learned_probabilities),
            },
        },
        "base_target_total_variation": base_target_total_variation,
        "learned_target_total_variation": learned_target_total_variation,
        "candidates": candidates,
    });
    Ok(ReanalysisAudit {
        report,
        source: decision.source.clone(),
        contains_budget_unknown,
        contains_exact_non_win,
        base_exact_win_mass,
        learned_exact_win_mass,
        target_exact_win_mass,
        base_target_total_variation,
        learned_target_total_variation,
    })
}

fn ranks(weights: &[f64]) -> Vec<usize> {
    weights
        .iter()
        .enumerate()
        .map(|(index, weight)| {
            1 + weights
                .iter()
                .enumerate()
                .filter(|(other_index, other)| {
                    other.total_cmp(weight).is_gt()
                        || (other.total_cmp(weight).is_eq() && *other_index < index)
                })
                .count()
        })
        .collect()
}

fn normalized_probabilities(weights: &[f64]) -> Vec<f64> {
    let safe = weights
        .iter()
        .map(|weight| {
            if weight.is_finite() && *weight > 0.0 {
                *weight
            } else {
                1.0
            }
        })
        .collect::<Vec<_>>();
    let max_weight = safe.iter().copied().fold(f64::MIN_POSITIVE, f64::max);
    let scaled_total = safe
        .iter()
        .map(|weight| *weight / max_weight)
        .sum::<f64>()
        .max(f64::MIN_POSITIVE);
    safe.into_iter()
        .map(|weight| (weight / max_weight) / scaled_total)
        .collect()
}

fn evidence_name(evidence: CombatActionReanalysisEvidenceV1) -> &'static str {
    match evidence {
        CombatActionReanalysisEvidenceV1::ExactWin { .. } => "exact_win",
        CombatActionReanalysisEvidenceV1::ExactNonWin => "exact_non_win",
        CombatActionReanalysisEvidenceV1::BudgetUnknown => "budget_unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;
    use sts_oracle_runtime::eval::combat_action_imitation::{
        conservative_combat_reanalysis_target_v1, CombatActionReanalysisEvidenceV1,
        CombatActionReanalysisTrainingConfigV1,
    };

    use super::{promotion_gate_failure_reason, ReanalysisAudit};

    #[test]
    fn conservative_target_preserves_unknown_mass_and_only_zeros_exact_non_wins() {
        let evidence = [
            CombatActionReanalysisEvidenceV1::ExactWin { final_hp: 14 },
            CombatActionReanalysisEvidenceV1::BudgetUnknown,
            CombatActionReanalysisEvidenceV1::ExactNonWin,
            CombatActionReanalysisEvidenceV1::ExactWin { final_hp: 8 },
        ];
        let target = conservative_combat_reanalysis_target_v1(
            &[8.0, 4.0, 2.0, 1.0],
            &evidence,
            CombatActionReanalysisTrainingConfigV1 {
                exact_support_mass: 0.5,
            },
        )
        .expect("typed target");

        assert!((target.iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
        assert!(target[1] > 0.0, "budget unknown must retain base mass");
        assert_eq!(target[2], 0.0, "only exact non-wins lose all mass");
        assert!(target[0] > 0.5 * 8.0 / 13.0);
        assert!(target[3] > 0.5 * 1.0 / 13.0);
    }

    #[test]
    fn conservative_target_requires_exact_support() {
        let result = conservative_combat_reanalysis_target_v1(
            &[1.0, 1.0],
            &[
                CombatActionReanalysisEvidenceV1::BudgetUnknown,
                CombatActionReanalysisEvidenceV1::ExactNonWin,
            ],
            CombatActionReanalysisTrainingConfigV1::default(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn all_exact_wins_preserve_the_base_distribution() {
        let evidence = [
            CombatActionReanalysisEvidenceV1::ExactWin { final_hp: 9 },
            CombatActionReanalysisEvidenceV1::ExactWin { final_hp: 30 },
            CombatActionReanalysisEvidenceV1::ExactWin { final_hp: 30 },
        ];
        let target = conservative_combat_reanalysis_target_v1(
            &[8.0, 3.0, 1.0],
            &evidence,
            CombatActionReanalysisTrainingConfigV1 {
                exact_support_mass: 0.5,
            },
        )
        .expect("all-win target");

        assert!((target[0] - 8.0 / 12.0).abs() < 1.0e-12);
        assert!((target[1] - 3.0 / 12.0).abs() < 1.0e-12);
        assert!((target[2] - 1.0 / 12.0).abs() < 1.0e-12);
    }

    #[test]
    fn promotion_gate_requires_each_learned_distribution_to_improve_on_base() {
        let audit = |contains_budget_unknown,
                     contains_exact_non_win,
                     base_exact_win_mass,
                     learned_exact_win_mass,
                     base_target_total_variation,
                     learned_target_total_variation| {
            ReanalysisAudit {
                report: json!({}),
                source: PathBuf::from("source.json"),
                contains_budget_unknown,
                contains_exact_non_win,
                base_exact_win_mass,
                learned_exact_win_mass,
                target_exact_win_mass: 0.75,
                base_target_total_variation,
                learned_target_total_variation,
            }
        };
        assert_eq!(
            promotion_gate_failure_reason(&audit(true, false, 0.4, 0.2, 0.3, 0.2)),
            Some("learned_exact_win_mass_below_base_with_budget_unknown")
        );
        assert_eq!(
            promotion_gate_failure_reason(&audit(false, false, 1.0, 1.0, 0.0, 0.4)),
            None,
            "all exact wins provide no ordering evidence"
        );
        assert_eq!(
            promotion_gate_failure_reason(&audit(false, true, 0.4, 0.6, 0.3, 0.4)),
            Some("learned_policy_farther_from_typed_reanalysis_target_than_base")
        );
        assert_eq!(
            promotion_gate_failure_reason(&audit(true, false, 0.4, 0.6, 0.3, 0.2)),
            None
        );
    }
}
