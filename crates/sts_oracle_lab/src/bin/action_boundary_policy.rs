//! Conservative action-policy training from complete next-boundary evidence.
//!
//! A frozen value artifact may add support to the best exactly generated
//! boundary successors.  Lower-valued exact boundaries and BudgetUnknown
//! actions retain base-policy mass; only exact non-wins are removed.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Args;
use serde::Deserialize;
use serde_json::{json, Value};
use sts_combat_planner::{CombatActionPolicy, CombatPolicyChoice};
use sts_oracle_runtime::eval::combat_action_imitation::{
    audit_combat_action_imitation_v1, combat_action_imitation_policy_v1,
    train_combat_action_imitation_with_soft_targets_and_initial_v1,
    CombatActionImitationArtifactV1, CombatActionImitationCoefficientV1,
    CombatActionImitationDemonstrationV1, CombatActionImitationTrainingConfigV1,
    CombatActionReanalysisTrainingConfigV1, CombatActionSoftTargetDecisionV1,
};
use sts_oracle_runtime::eval::combat_guidance_bundle::CombatGuidanceBundleV1;
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::CombatPosition;
use sts_oracle_runtime::state::core::ClientInput;

use super::exact_turn_corridor::load_corpus as load_combat_action_imitation_corpus;
use super::source_content_fingerprint;

const CORPUS_SCHEMA: &str = "ActionBoundaryEvidenceCorpusV1";
const REFINEMENT_LINE_SEARCH_STEPS: usize = 64;

#[derive(Debug, Args)]
pub(super) struct ActionBoundaryPolicyArgs {
    /// Exact-witness training manifest. Evidence roots replace their ordinary
    /// imitation examples rather than being counted twice.
    #[arg(long)]
    manifest: PathBuf,
    /// Complete or explicitly BudgetUnknown action-boundary evidence corpora.
    #[arg(long)]
    boundary_corpus: Vec<PathBuf>,
    /// Batch reports produced by `build-action-boundary-evidence-batch`.
    /// Incomplete structured surfaces are reported and excluded explicitly.
    #[arg(long)]
    boundary_batch_report: Vec<PathBuf>,
    /// Exact witnesses excluded from training. They do not influence fitting
    /// or line-search selection; promotion is rejected if the candidate adds
    /// action-ranking misses relative to the frozen incumbent.
    #[arg(long)]
    validation_manifest: Vec<PathBuf>,
    /// Frozen bundle used while producing every boundary corpus.
    #[arg(long)]
    guidance_bundle: PathBuf,
    /// Destination for the newly trained action artifact.
    #[arg(long)]
    output_action: PathBuf,
    /// Destination for the new action artifact paired with the unchanged
    /// frozen boundary value artifact.
    #[arg(long)]
    output_bundle: PathBuf,
    /// Durable build and promotion report, written for accepted and rejected
    /// candidates alike.
    #[arg(long)]
    report: PathBuf,
    /// Probability mass added to the best exact boundary support while all
    /// non-refuted alternatives retain positive base mass.
    #[arg(long, default_value_t = 0.5)]
    preferred_support_mass: f64,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

#[derive(Debug, Deserialize)]
struct BoundaryCorpus {
    schema_name: String,
    schema_version: u32,
    guidance_bundle_content_fingerprint: String,
    root_position: CombatPosition,
    surface: BoundarySurface,
    candidates: Vec<BoundaryCandidate>,
}

#[derive(Debug, Deserialize)]
struct BoundarySurface {
    complete: bool,
    materialized_candidates: usize,
    atomic_actions: usize,
    structured_family_count: usize,
}

#[derive(Debug, Deserialize)]
struct BoundaryCandidate {
    input: ClientInput,
    label: String,
    base_policy_probability: f64,
    guided_policy_probability: f64,
    evidence: BoundaryEvidence,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BoundaryEvidence {
    ExactTerminalWin {},
    ExactBoundarySuccessor { successor: BoundaryObservation },
    ExactNonWin {},
    BudgetUnknown {},
}

#[derive(Debug, Deserialize)]
struct BoundaryObservation {
    value_target_available: bool,
    value_rank: Vec<i32>,
}

#[derive(Debug, Deserialize)]
struct BoundaryBatchReport {
    schema_name: String,
    schema_version: u32,
    generated: Vec<BoundaryBatchEntry>,
}

#[derive(Debug, Deserialize)]
struct BoundaryBatchEntry {
    output: PathBuf,
}

struct LoadedBoundaryDecision {
    source: PathBuf,
    root: CombatPosition,
    candidates: Vec<ClientInput>,
    labels: Vec<String>,
    target_probabilities: Vec<f64>,
    top1_accepted_indices: Vec<usize>,
    preferred_indices: Vec<usize>,
    unknown_indices: Vec<usize>,
    exact_non_win_indices: Vec<usize>,
}

struct BoundaryAudit {
    source: PathBuf,
    report: Value,
    base_target_total_variation: f64,
    learned_target_total_variation: f64,
    base_preferred_mass: f64,
    learned_preferred_mass: f64,
}

pub(super) fn build(args: ActionBoundaryPolicyArgs) -> Result<Value, String> {
    if args.max_engine_steps_per_transition == 0
        || !args.preferred_support_mass.is_finite()
        || !(0.0..1.0).contains(&args.preferred_support_mass)
    {
        return Err("invalid action-boundary policy training configuration".to_string());
    }
    if args.boundary_corpus.is_empty() && args.boundary_batch_report.is_empty() {
        return Err("provide --boundary-corpus or --boundary-batch-report".to_string());
    }
    let bundle = CombatGuidanceBundleV1::load(&args.guidance_bundle)?;
    let bundle_fingerprint = source_content_fingerprint(
        &std::env::current_dir().map_err(|error| error.to_string())?,
        std::slice::from_ref(&args.guidance_bundle),
    )?;
    let demonstrations = load_combat_action_imitation_corpus(&args.manifest)?;
    let validation_demonstrations = args
        .validation_manifest
        .iter()
        .map(|manifest| load_combat_action_imitation_corpus(manifest))
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut corpus_paths = args.boundary_corpus.clone();
    for report_path in &args.boundary_batch_report {
        let report = serde_json::from_slice::<BoundaryBatchReport>(
            &std::fs::read(report_path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| {
            format!(
                "invalid boundary batch report {}: {error}",
                report_path.display()
            )
        })?;
        if report.schema_name != "ActionBoundaryEvidenceBatchReportV1" || report.schema_version != 1
        {
            return Err(format!(
                "unsupported boundary batch report {}",
                report_path.display()
            ));
        }
        corpus_paths.extend(report.generated.into_iter().map(|entry| entry.output));
    }
    corpus_paths.sort();
    corpus_paths.dedup();
    let mut loaded = Vec::new();
    let mut excluded_incomplete_surfaces = Vec::new();
    for path in &corpus_paths {
        match load_boundary_corpus(path, &bundle_fingerprint, args.preferred_support_mass)? {
            Some(decision) => loaded.push(decision),
            None => excluded_incomplete_surfaces.push(path.clone()),
        }
    }
    let informative = loaded
        .iter()
        .filter(|decision| !decision.preferred_indices.is_empty())
        .collect::<Vec<_>>();
    if informative.is_empty() {
        return Err("action-boundary corpora contain no informative typed targets".to_string());
    }

    let borrowed_demonstrations = demonstrations
        .iter()
        .map(|demonstration| CombatActionImitationDemonstrationV1 {
            root: &demonstration.position,
            actions: &demonstration.actions,
        })
        .collect::<Vec<_>>();
    let training_config = CombatActionImitationTrainingConfigV1 {
        max_engine_steps_per_transition: args.max_engine_steps_per_transition,
        base_weight_exponent: 1.0,
        ..CombatActionImitationTrainingConfigV1::default()
    };
    let training_base_policy = retained_combat_tactical_policy_v1();
    let incumbent_policy = combat_action_imitation_policy_v1(
        training_base_policy.clone(),
        bundle.action_imitation.clone(),
    )?;
    let mut active_indices = (0..informative.len()).collect::<Vec<_>>();
    let mut initial_artifact = bundle.action_imitation.clone();
    let mut previous_failure_count = usize::MAX;
    let mut best = None::<(
        CombatActionImitationArtifactV1,
        Vec<BoundaryAudit>,
        Vec<usize>,
        f64,
    )>;
    let mut refinement_rounds = Vec::new();
    for round in 0..=informative.len() {
        let borrowed_targets = active_indices
            .iter()
            .map(|index| {
                let decision = informative[*index];
                CombatActionSoftTargetDecisionV1 {
                    root: &decision.root,
                    candidates: &decision.candidates,
                    target_probabilities: &decision.target_probabilities,
                    top1_accepted_indices: &decision.top1_accepted_indices,
                }
            })
            .collect::<Vec<_>>();
        let trained_candidate = train_combat_action_imitation_with_soft_targets_and_initial_v1(
            &borrowed_demonstrations,
            &[],
            &borrowed_targets,
            training_config,
            CombatActionReanalysisTrainingConfigV1::default(),
            training_base_policy.clone(),
            Some(&initial_artifact),
        )?;
        let endpoint = retain_contextual_boundary_updates(&initial_artifact, trained_candidate);
        let mut segment_best = None::<(
            CombatActionImitationArtifactV1,
            Vec<BoundaryAudit>,
            Vec<usize>,
            f64,
            f64,
        )>;
        for step in 1..=REFINEMENT_LINE_SEARCH_STEPS {
            let alpha = step as f64 / REFINEMENT_LINE_SEARCH_STEPS as f64;
            let candidate = interpolate_artifacts(&initial_artifact, &endpoint, alpha);
            let learned_policy =
                combat_action_imitation_policy_v1(training_base_policy.clone(), candidate.clone())?;
            let candidate_audits = informative
                .iter()
                .map(|decision| {
                    audit_boundary_decision(
                        decision,
                        incumbent_policy.as_ref(),
                        learned_policy.as_ref(),
                    )
                })
                .collect::<Result<Vec<_>, String>>()?;
            let failure_indices = candidate_audits
                .iter()
                .enumerate()
                .filter_map(|(index, audit)| boundary_failure_reason(audit).map(|_| index))
                .collect::<Vec<_>>();
            let total_variation = candidate_audits
                .iter()
                .map(|audit| audit.learned_target_total_variation)
                .sum::<f64>();
            let better = segment_best
                .as_ref()
                .map(|(_, _, best_failures, best_total_variation, best_alpha)| {
                    failure_indices.len() < best_failures.len()
                        || (failure_indices.len() == best_failures.len()
                            && (total_variation < *best_total_variation - 1.0e-12
                                || ((total_variation - *best_total_variation).abs() <= 1.0e-12
                                    && alpha > *best_alpha)))
                })
                .unwrap_or(true);
            if better {
                segment_best = Some((
                    candidate,
                    candidate_audits,
                    failure_indices,
                    total_variation,
                    alpha,
                ));
            }
        }
        let (candidate, candidate_audits, failure_indices, total_variation, selected_alpha) =
            segment_best.expect("refinement line search always evaluates at least one step");
        refinement_rounds.push(json!({
            "round": round + 1,
            "active_target_count": active_indices.len(),
            "failure_count": failure_indices.len(),
            "failure_sources": failure_indices
                .iter()
                .map(|index| &informative[*index].source)
                .collect::<Vec<_>>(),
            "failure_diagnostics": failure_indices
                .iter()
                .map(|index| {
                    let audit = &candidate_audits[*index];
                    json!({
                        "source": audit.source,
                        "reason": boundary_failure_reason(audit),
                        "base_target_total_variation": audit.base_target_total_variation,
                        "learned_target_total_variation": audit.learned_target_total_variation,
                        "base_preferred_mass": audit.base_preferred_mass,
                        "learned_preferred_mass": audit.learned_preferred_mass,
                    })
                })
                .collect::<Vec<_>>(),
            "mean_target_total_variation": total_variation / candidate_audits.len() as f64,
            "line_search_steps": REFINEMENT_LINE_SEARCH_STEPS,
            "selected_alpha": selected_alpha,
            "active_sources": active_indices
                .iter()
                .map(|index| &informative[*index].source)
                .collect::<Vec<_>>(),
        }));
        let better = best
            .as_ref()
            .map(|(_, _, best_failures, best_total_variation)| {
                failure_indices.len() < best_failures.len()
                    || (failure_indices.len() == best_failures.len()
                        && total_variation < *best_total_variation)
            })
            .unwrap_or(true);
        if better {
            best = Some((
                candidate.clone(),
                candidate_audits,
                failure_indices.clone(),
                total_variation,
            ));
        }
        if failure_indices.is_empty() || failure_indices.len() >= previous_failure_count {
            break;
        }
        previous_failure_count = failure_indices.len();
        initial_artifact = candidate;
        active_indices = failure_indices;
    }
    let (artifact, audits, _, _) = best.expect("at least one refinement round always runs");
    let failures = audits
        .iter()
        .filter_map(|audit| {
            let reason = boundary_failure_reason(audit)?;
            Some(json!({
                "source": audit.source,
                "reason": reason,
                "base_target_total_variation": audit.base_target_total_variation,
                "learned_target_total_variation": audit.learned_target_total_variation,
                "base_preferred_mass": audit.base_preferred_mass,
                "learned_preferred_mass": audit.learned_preferred_mass,
            }))
        })
        .collect::<Vec<_>>();
    let validation = validation_demonstrations
        .iter()
        .map(|demonstration| {
            let incumbent = audit_combat_action_imitation_v1(
                &demonstration.position,
                &demonstration.actions,
                &bundle.action_imitation,
                training_base_policy.as_ref(),
                training_config.max_structured_alternatives,
                args.max_engine_steps_per_transition,
            )?;
            let candidate = audit_combat_action_imitation_v1(
                &demonstration.position,
                &demonstration.actions,
                &artifact,
                training_base_policy.as_ref(),
                training_config.max_structured_alternatives,
                args.max_engine_steps_per_transition,
            )?;
            Ok::<_, String>(json!({
                "id": demonstration.id,
                "ranked_decision_count": candidate.ranked_decision_count,
                "incumbent_miss_count": incumbent.misses.len(),
                "candidate_miss_count": candidate.misses.len(),
                "regressed": candidate.misses.len() > incumbent.misses.len(),
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let validation_regressions = validation
        .iter()
        .filter(|audit| audit["regressed"].as_bool() == Some(true))
        .cloned()
        .collect::<Vec<_>>();
    let artifact_saved = failures.is_empty() && validation_regressions.is_empty();
    if artifact_saved {
        artifact.save(&args.output_action)?;
        CombatGuidanceBundleV1::new(
            "typed_action_boundary_policy_with_frozen_boundary_value",
            artifact.clone(),
            bundle.boundary_value.clone(),
        )?
        .save(&args.output_bundle)?;
    } else {
        for path in [&args.output_action, &args.output_bundle] {
            if path.exists() {
                std::fs::remove_file(path).map_err(|error| {
                    format!(
                        "failed to remove rejected artifact {}: {error}",
                        path.display()
                    )
                })?;
            }
        }
    }

    let demonstration_audits = demonstrations
        .iter()
        .map(|demonstration| {
            audit_combat_action_imitation_v1(
                &demonstration.position,
                &demonstration.actions,
                &artifact,
                training_base_policy.as_ref(),
                training_config.max_structured_alternatives,
                args.max_engine_steps_per_transition,
            )
            .map(|audit| {
                json!({
                    "id": demonstration.id,
                    "ranked_decision_count": audit.ranked_decision_count,
                    "miss_count": audit.misses.len(),
                })
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let report = json!({
        "schema_name": "ActionBoundaryPolicyBuildReportV1",
        "schema_version": 1,
        "manifest": args.manifest,
        "boundary_corpora": corpus_paths,
        "boundary_batch_reports": args.boundary_batch_report,
        "validation_manifests": args.validation_manifest,
        "excluded_incomplete_surfaces": excluded_incomplete_surfaces,
        "guidance_bundle": args.guidance_bundle,
        "guidance_bundle_content_fingerprint": bundle_fingerprint,
        "output_action": args.output_action,
        "output_bundle": args.output_bundle,
        "preferred_support_mass": args.preferred_support_mass,
        "training_base": "existing_combat_knowledge_v1",
        "promotion_baseline": "frozen_guidance_bundle_action_imitation_over_existing_combat_knowledge_v1",
        "promotion": {
            "status": if artifact_saved { "accepted" } else { "rejected" },
            "artifact_saved": artifact_saved,
            "contract": "each_informative_boundary_decision_closer_to_target_and_no_preferred_mass_regression_and_no_validation_witness_ranking_regression",
            "failures": failures,
            "validation_regressions": validation_regressions,
            "refinement_rounds": refinement_rounds,
        },
        "artifact": {
            "training_authority": artifact.training_authority,
            "source_trajectory_count": artifact.source_trajectory_count,
            "source_action_count": artifact.source_action_count,
            "boundary_decision_count": informative.len(),
            "skipped_boundary_decision_count": loaded.len().saturating_sub(informative.len()),
            "ranked_decision_count": artifact.ranked_decision_count,
            "pairwise_comparison_count": artifact.pairwise_comparison_count,
            "training_top1_correct": artifact.training_top1_correct,
            "training_top1_total": artifact.training_top1_total,
            "coefficient_count": artifact.coefficients.len(),
        },
        "demonstrations": demonstration_audits,
        "validation": validation,
        "boundary_decisions": audits.into_iter().map(|audit| audit.report).collect::<Vec<_>>(),
    });
    if let Some(parent) = args.report.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        &args.report,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(report)
}

fn retain_contextual_boundary_updates(
    initial: &CombatActionImitationArtifactV1,
    mut trained: CombatActionImitationArtifactV1,
) -> CombatActionImitationArtifactV1 {
    let mut coefficients = initial
        .coefficients
        .iter()
        .map(|coefficient| (coefficient.feature.clone(), coefficient.clone()))
        .collect::<BTreeMap<_, _>>();
    for coefficient in &trained.coefficients {
        if coefficient.feature.starts_with("cross/") {
            coefficients.insert(coefficient.feature.clone(), coefficient.clone());
        }
    }
    trained.coefficients = coefficients.into_values().collect();
    trained
}

fn interpolate_artifacts(
    initial: &CombatActionImitationArtifactV1,
    trained: &CombatActionImitationArtifactV1,
    alpha: f64,
) -> CombatActionImitationArtifactV1 {
    debug_assert!((0.0..=1.0).contains(&alpha));
    let initial_weights = initial
        .coefficients
        .iter()
        .map(|coefficient| (coefficient.feature.as_str(), coefficient.weight))
        .collect::<BTreeMap<_, _>>();
    let trained_weights = trained
        .coefficients
        .iter()
        .map(|coefficient| (coefficient.feature.as_str(), coefficient.weight))
        .collect::<BTreeMap<_, _>>();
    let mut features = initial_weights
        .keys()
        .chain(trained_weights.keys())
        .copied()
        .collect::<Vec<_>>();
    features.sort_unstable();
    features.dedup();

    let mut interpolated = trained.clone();
    interpolated.coefficients = features
        .into_iter()
        .filter_map(|feature| {
            let initial_weight = initial_weights.get(feature).copied().unwrap_or(0.0);
            let trained_weight = trained_weights.get(feature).copied().unwrap_or(0.0);
            let weight = initial_weight + alpha * (trained_weight - initial_weight);
            (weight.abs() >= 1.0e-10).then(|| CombatActionImitationCoefficientV1 {
                feature: feature.to_string(),
                weight,
            })
        })
        .collect();
    interpolated
}

fn load_boundary_corpus(
    path: &PathBuf,
    expected_bundle_fingerprint: &str,
    preferred_support_mass: f64,
) -> Result<Option<LoadedBoundaryDecision>, String> {
    let corpus = serde_json::from_slice::<BoundaryCorpus>(
        &std::fs::read(path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("invalid action-boundary corpus {}: {error}", path.display()))?;
    if corpus.schema_name != CORPUS_SCHEMA || corpus.schema_version != 1 {
        return Err(format!(
            "unsupported action-boundary corpus schema in {}",
            path.display()
        ));
    }
    if corpus.guidance_bundle_content_fingerprint != expected_bundle_fingerprint {
        return Err(format!(
            "action-boundary corpus {} used a different frozen guidance bundle",
            path.display()
        ));
    }
    if !corpus.surface.complete
        || corpus.surface.structured_family_count != 0
        || corpus.surface.materialized_candidates != corpus.candidates.len()
        || corpus.surface.atomic_actions != corpus.candidates.len()
    {
        return Ok(None);
    }
    let candidates = corpus
        .candidates
        .iter()
        .map(|candidate| candidate.input.clone())
        .collect::<Vec<_>>();
    let labels = corpus
        .candidates
        .iter()
        .map(|candidate| candidate.label.clone())
        .collect::<Vec<_>>();
    let exact_non_win_indices = corpus
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            matches!(candidate.evidence, BoundaryEvidence::ExactNonWin {}).then_some(index)
        })
        .collect::<Vec<_>>();
    let unknown_indices = corpus
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            matches!(candidate.evidence, BoundaryEvidence::BudgetUnknown {}).then_some(index)
        })
        .collect::<Vec<_>>();
    let preferred_indices = preferred_boundary_indices(&corpus.candidates, &unknown_indices);
    if corpus.candidates.iter().any(|candidate| {
        !candidate.base_policy_probability.is_finite()
            || candidate.base_policy_probability <= 0.0
            || !candidate.guided_policy_probability.is_finite()
            || candidate.guided_policy_probability <= 0.0
    }) {
        return Err(format!(
            "action-boundary corpus {} has invalid base probability",
            path.display()
        ));
    }
    let eligible_count = corpus
        .candidates
        .len()
        .saturating_sub(exact_non_win_indices.len());
    if eligible_count == 0 {
        return Err(format!(
            "action-boundary corpus {} refutes every legal action",
            path.display()
        ));
    }
    let informative = !preferred_indices.is_empty()
        && (preferred_indices.len() < eligible_count || !exact_non_win_indices.is_empty());
    let support_mass = if informative {
        preferred_support_mass
    } else {
        0.0
    };
    let base_probabilities = corpus
        .candidates
        .iter()
        .map(|candidate| candidate.guided_policy_probability)
        .collect::<Vec<_>>();
    let target_probabilities = conservative_boundary_target(
        &base_probabilities,
        &exact_non_win_indices,
        &preferred_indices,
        support_mass,
    );
    let best_target = target_probabilities
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let top1_accepted_indices = target_probabilities
        .iter()
        .enumerate()
        .filter_map(|(index, probability)| {
            ((*probability - best_target).abs() <= 1.0e-12).then_some(index)
        })
        .collect::<Vec<_>>();
    Ok(Some(LoadedBoundaryDecision {
        source: path.clone(),
        root: corpus.root_position,
        candidates,
        labels,
        target_probabilities,
        top1_accepted_indices,
        preferred_indices: informative.then_some(preferred_indices).unwrap_or_default(),
        unknown_indices,
        exact_non_win_indices,
    }))
}

fn preferred_boundary_indices(
    candidates: &[BoundaryCandidate],
    unknown_indices: &[usize],
) -> Vec<usize> {
    let terminal_wins = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            matches!(candidate.evidence, BoundaryEvidence::ExactTerminalWin {}).then_some(index)
        })
        .collect::<Vec<_>>();
    if !terminal_wins.is_empty() {
        return terminal_wins;
    }
    if !unknown_indices.is_empty() {
        return Vec::new();
    }

    let ranked = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| match &candidate.evidence {
            BoundaryEvidence::ExactBoundarySuccessor { successor }
                if successor.value_target_available =>
            {
                Some((index, successor.value_rank.as_slice()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let best = ranked.iter().map(|(_, rank)| *rank).max();
    ranked
        .into_iter()
        .filter_map(|(index, rank)| (Some(rank) == best).then_some(index))
        .collect()
}

fn conservative_boundary_target(
    base_probabilities: &[f64],
    exact_non_win_indices: &[usize],
    preferred_indices: &[usize],
    preferred_support_mass: f64,
) -> Vec<f64> {
    let eligible_total = base_probabilities
        .iter()
        .enumerate()
        .filter(|(index, _)| !exact_non_win_indices.contains(index))
        .map(|(_, probability)| *probability)
        .sum::<f64>();
    let mut target = base_probabilities
        .iter()
        .enumerate()
        .map(|(index, probability)| {
            if exact_non_win_indices.contains(&index) {
                0.0
            } else {
                (1.0 - preferred_support_mass) * probability / eligible_total
            }
        })
        .collect::<Vec<_>>();
    if preferred_support_mass > 0.0 {
        for index in preferred_indices {
            target[*index] += preferred_support_mass / preferred_indices.len() as f64;
        }
    }
    target
}

fn audit_boundary_decision(
    decision: &LoadedBoundaryDecision,
    base_policy: &dyn CombatActionPolicy,
    learned_policy: &dyn CombatActionPolicy,
) -> Result<BoundaryAudit, String> {
    let choices = decision
        .candidates
        .iter()
        .map(CombatPolicyChoice::Atomic)
        .collect::<Vec<_>>();
    let base = normalized_probabilities(&base_policy.weights(&decision.root, &choices));
    let learned = normalized_probabilities(&learned_policy.weights(&decision.root, &choices));
    if base.len() != decision.candidates.len() || learned.len() != decision.candidates.len() {
        return Err(format!(
            "action-boundary audit received misaligned policy weights for {}",
            decision.source.display()
        ));
    }
    let mass = |probabilities: &[f64], indices: &[usize]| {
        indices
            .iter()
            .map(|index| probabilities[*index])
            .sum::<f64>()
    };
    let total_variation = |left: &[f64], right: &[f64]| {
        0.5 * left
            .iter()
            .zip(right)
            .map(|(left, right)| (left - right).abs())
            .sum::<f64>()
    };
    let base_target_total_variation = total_variation(&base, &decision.target_probabilities);
    let learned_target_total_variation = total_variation(&learned, &decision.target_probabilities);
    let base_preferred_mass = mass(&base, &decision.preferred_indices);
    let learned_preferred_mass = mass(&learned, &decision.preferred_indices);
    let candidates = decision
        .candidates
        .iter()
        .enumerate()
        .map(|(index, input)| {
            json!({
                "label": decision.labels[index],
                "input": input,
                "preferred_exact_boundary": decision.preferred_indices.contains(&index),
                "budget_unknown": decision.unknown_indices.contains(&index),
                "exact_non_win": decision.exact_non_win_indices.contains(&index),
                "base_probability": base[index],
                "target_probability": decision.target_probabilities[index],
                "learned_probability": learned[index],
            })
        })
        .collect::<Vec<_>>();
    Ok(BoundaryAudit {
        source: decision.source.clone(),
        report: json!({
            "source": decision.source,
            "candidate_count": decision.candidates.len(),
            "preferred_indices": decision.preferred_indices,
            "unknown_indices": decision.unknown_indices,
            "exact_non_win_indices": decision.exact_non_win_indices,
            "base_target_total_variation": base_target_total_variation,
            "learned_target_total_variation": learned_target_total_variation,
            "mass": {
                "base_preferred": base_preferred_mass,
                "target_preferred": mass(&decision.target_probabilities, &decision.preferred_indices),
                "learned_preferred": learned_preferred_mass,
                "base_unknown": mass(&base, &decision.unknown_indices),
                "target_unknown": mass(&decision.target_probabilities, &decision.unknown_indices),
                "learned_unknown": mass(&learned, &decision.unknown_indices),
            },
            "candidates": candidates,
        }),
        base_target_total_variation,
        learned_target_total_variation,
        base_preferred_mass,
        learned_preferred_mass,
    })
}

fn boundary_failure_reason(audit: &BoundaryAudit) -> Option<&'static str> {
    if audit.learned_target_total_variation > audit.base_target_total_variation + 1.0e-12 {
        Some("learned_policy_farther_from_typed_boundary_target_than_base")
    } else if audit.learned_preferred_mass + 1.0e-12 < audit.base_preferred_mass {
        Some("learned_preferred_boundary_mass_below_base")
    } else {
        None
    }
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
    let total = safe.iter().sum::<f64>().max(f64::MIN_POSITIVE);
    safe.into_iter().map(|weight| weight / total).collect()
}

#[cfg(test)]
mod tests {
    use sts_oracle_runtime::eval::combat_action_imitation::{
        CombatActionImitationArtifactV1, CombatActionImitationCoefficientV1,
    };
    use sts_oracle_runtime::state::core::ClientInput;

    use super::{
        conservative_boundary_target, interpolate_artifacts, normalized_probabilities,
        preferred_boundary_indices, retain_contextual_boundary_updates, BoundaryCandidate,
        BoundaryEvidence, BoundaryObservation,
    };

    fn artifact(coefficients: &[(&str, f64)]) -> CombatActionImitationArtifactV1 {
        CombatActionImitationArtifactV1 {
            schema_name: "test".to_string(),
            schema_version: 1,
            feature_schema: "test".to_string(),
            runtime_compatibility_id: "test".to_string(),
            training_authority: "test".to_string(),
            source_trajectory_count: 1,
            source_action_count: 1,
            source_terminal_final_hp: 1,
            ranked_decision_count: 1,
            pairwise_comparison_count: 1,
            skipped_forced_decision_count: 0,
            training_top1_correct: 1,
            training_top1_total: 1,
            logit_scale: 1.0,
            max_abs_log_factor: 3.0,
            base_weight_exponent: 1.0,
            coefficients: coefficients
                .iter()
                .map(|(feature, weight)| CombatActionImitationCoefficientV1 {
                    feature: (*feature).to_string(),
                    weight: *weight,
                })
                .collect(),
        }
    }

    fn candidate(evidence: BoundaryEvidence) -> BoundaryCandidate {
        BoundaryCandidate {
            input: ClientInput::EndTurn,
            label: "test".to_string(),
            base_policy_probability: 0.5,
            guided_policy_probability: 0.5,
            evidence,
        }
    }

    #[test]
    fn normalization_keeps_every_non_refuted_weight_positive() {
        let probabilities = normalized_probabilities(&[8.0, 0.0, f64::NAN, 2.0]);

        assert!((probabilities.iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
        assert!(probabilities.iter().all(|probability| *probability > 0.0));
    }

    #[test]
    fn conservative_target_rejects_only_proven_losses_and_keeps_unknown_support() {
        // Candidate 0 is the best exact next boundary, 1 is BudgetUnknown,
        // and 2 is an exact non-win.  Unknown evidence must not be converted
        // into a loss merely because candidate 0 has stronger local evidence.
        let target = conservative_boundary_target(&[0.1, 0.2, 0.7], &[2], &[0], 0.5);

        assert!((target.iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
        assert!(target[0] > 0.5);
        assert!(target[1] > 0.0);
        assert_eq!(target[2], 0.0);
    }

    #[test]
    fn unknown_successor_blocks_value_rank_from_becoming_exact_preference() {
        let candidates = vec![
            candidate(BoundaryEvidence::ExactBoundarySuccessor {
                successor: BoundaryObservation {
                    value_target_available: true,
                    value_rank: vec![9],
                },
            }),
            candidate(BoundaryEvidence::BudgetUnknown {}),
        ];

        assert!(preferred_boundary_indices(&candidates, &[1]).is_empty());
    }

    #[test]
    fn exact_terminal_win_remains_preferred_when_other_actions_are_unknown() {
        let candidates = vec![
            candidate(BoundaryEvidence::ExactTerminalWin {}),
            candidate(BoundaryEvidence::BudgetUnknown {}),
        ];

        assert_eq!(preferred_boundary_indices(&candidates, &[1]), vec![0]);
    }

    #[test]
    fn interpolation_uses_zero_for_features_missing_from_either_endpoint() {
        let initial = artifact(&[("cross/a", 2.0), ("cross/b", 4.0)]);
        let trained = artifact(&[("cross/a", 6.0), ("cross/c", 8.0)]);
        let interpolated = interpolate_artifacts(&initial, &trained, 0.25);
        let weights = interpolated
            .coefficients
            .iter()
            .map(|coefficient| (coefficient.feature.as_str(), coefficient.weight))
            .collect::<std::collections::BTreeMap<_, _>>();

        assert_eq!(weights["cross/a"], 3.0);
        assert_eq!(weights["cross/b"], 3.0);
        assert_eq!(weights["cross/c"], 2.0);
    }

    #[test]
    fn contextual_update_preserves_every_non_cross_incumbent_weight() {
        let initial = artifact(&[("action/global", 2.0), ("cross/a", 3.0)]);
        let trained = artifact(&[("action/global", 9.0), ("cross/a", 4.0), ("cross/b", 5.0)]);
        let retained = retain_contextual_boundary_updates(&initial, trained);
        let weights = retained
            .coefficients
            .iter()
            .map(|coefficient| (coefficient.feature.as_str(), coefficient.weight))
            .collect::<std::collections::BTreeMap<_, _>>();

        assert_eq!(weights["action/global"], 2.0);
        assert_eq!(weights["cross/a"], 4.0);
        assert_eq!(weights["cross/b"], 5.0);
    }
}
