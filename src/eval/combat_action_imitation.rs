use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sts_combat_planner::{
    CombatActionPolicy, CombatPolicyChoice, CombatStateGuide, CombatStateGuideRank,
    SharedCombatActionPolicy, UniformCombatActionPolicy,
};

use crate::ai::analysis::card_semantics::{
    card_definition_with_upgrades as strategic_card_definition, PlayEffect, TriggeredEffect,
};
use crate::ai::combat_state_key::combat_exact_state_hash_v2;
use crate::content::cards::{get_card_definition, CardId};
use crate::content::powers::PowerId;
use crate::runtime::combat::CardLineageId;
use crate::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use crate::sim::combat_action::combat_action_key;
use crate::sim::combat_action_surface::CombatSelectionActionFamilyV2;
use crate::state::core::{ClientInput, EngineState};

pub const COMBAT_ACTION_IMITATION_SCHEMA_NAME: &str = "CombatActionImitationArtifactV1";
pub const COMBAT_ACTION_IMITATION_SCHEMA_VERSION: u32 = 4;
const COMBAT_ACTION_FEATURE_SCHEMA: &str = "typed-state-and-generation-x-semantic-action/v7";
const COMBAT_ACTION_IMITATION_RUNTIME_ID: &str = env!("STS_COMBAT_ACTION_IMITATION_RUNTIME_ID");

#[derive(Clone, Copy, Debug)]
pub struct CombatActionImitationTrainingConfigV1 {
    pub epochs: usize,
    pub learning_rate: f64,
    pub l2_penalty: f64,
    pub max_structured_alternatives: usize,
    pub max_engine_steps_per_transition: usize,
    pub logit_scale: f64,
    pub max_abs_log_factor: f64,
    /// Zero lets the learned distribution own action ordering. A positive
    /// value trains and applies the learned logits as residual corrections to
    /// the same base action policy used at runtime.
    pub base_weight_exponent: f64,
}

impl Default for CombatActionImitationTrainingConfigV1 {
    fn default() -> Self {
        Self {
            epochs: 240,
            learning_rate: 0.08,
            l2_penalty: 1.0e-4,
            max_structured_alternatives: 256,
            max_engine_steps_per_transition: 512,
            logit_scale: 1.0,
            max_abs_log_factor: 3.0,
            base_weight_exponent: 0.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatActionImitationCoefficientV1 {
    pub feature: String,
    pub weight: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatActionImitationArtifactV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub feature_schema: String,
    /// Fingerprint of the exact feature and action-surface implementation
    /// used to train this artifact. Unlike the human-authored feature schema,
    /// this changes automatically when its defining source files change.
    pub runtime_compatibility_id: String,
    pub training_authority: String,
    pub source_trajectory_count: usize,
    pub source_action_count: usize,
    pub source_terminal_final_hp: i32,
    pub ranked_decision_count: usize,
    pub pairwise_comparison_count: usize,
    pub skipped_forced_decision_count: usize,
    pub training_top1_correct: usize,
    pub training_top1_total: usize,
    pub logit_scale: f64,
    pub max_abs_log_factor: f64,
    pub base_weight_exponent: f64,
    pub coefficients: Vec<CombatActionImitationCoefficientV1>,
}

impl CombatActionImitationArtifactV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_name != COMBAT_ACTION_IMITATION_SCHEMA_NAME
            || self.schema_version != COMBAT_ACTION_IMITATION_SCHEMA_VERSION
            || self.feature_schema != COMBAT_ACTION_FEATURE_SCHEMA
        {
            return Err(format!(
                "unsupported combat action imitation schema: found {}/{}/{}, expected {}/{}/{}; rebuild the artifact",
                self.schema_name,
                self.schema_version,
                self.feature_schema,
                COMBAT_ACTION_IMITATION_SCHEMA_NAME,
                COMBAT_ACTION_IMITATION_SCHEMA_VERSION,
                COMBAT_ACTION_FEATURE_SCHEMA,
            ));
        }
        if self.runtime_compatibility_id != COMBAT_ACTION_IMITATION_RUNTIME_ID {
            return Err(format!(
                "combat action imitation runtime mismatch: found {:?}, expected {:?}; rebuild the artifact with the current binary",
                self.runtime_compatibility_id, COMBAT_ACTION_IMITATION_RUNTIME_ID,
            ));
        }
        if self.source_trajectory_count == 0
            || self.ranked_decision_count == 0
            || self.coefficients.is_empty()
        {
            return Err("combat action imitation artifact has no learned ranking".to_string());
        }
        if !self.logit_scale.is_finite() || self.logit_scale <= 0.0 {
            return Err(
                "combat action imitation logit scale must be positive and finite".to_string(),
            );
        }
        if !self.max_abs_log_factor.is_finite() || self.max_abs_log_factor <= 0.0 {
            return Err(
                "combat action imitation log-factor limit must be positive and finite".to_string(),
            );
        }
        if !self.base_weight_exponent.is_finite()
            || !(0.0..=1.0).contains(&self.base_weight_exponent)
        {
            return Err(
                "combat action imitation base-weight exponent must be in 0..=1".to_string(),
            );
        }
        if self
            .coefficients
            .iter()
            .any(|coefficient| coefficient.feature.is_empty() || !coefficient.weight.is_finite())
        {
            return Err("combat action imitation coefficient is invalid".to_string());
        }
        if self
            .coefficients
            .windows(2)
            .any(|pair| pair[0].feature >= pair[1].feature)
        {
            return Err(
                "combat action imitation coefficients must have unique ascending names".to_string(),
            );
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, bytes).map_err(|error| error.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let artifact = serde_json::from_slice::<Self>(
            &std::fs::read(path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("invalid combat action imitation artifact: {error}"))?;
        artifact.validate()?;
        Ok(artifact)
    }
}

type SparseFeatures = BTreeMap<String, f64>;

#[derive(Clone, Debug)]
struct RankingExample {
    target_probabilities: Vec<f64>,
    neutral_indices: Vec<usize>,
    top1_accepted_indices: Vec<usize>,
    candidates: Vec<SparseFeatures>,
    base_logits: Vec<f64>,
}

#[derive(Clone, Debug)]
struct IndexedRankingExample {
    target_probabilities: Vec<f64>,
    neutral_indices: Vec<usize>,
    candidates: Vec<Vec<(usize, f64)>>,
    base_logits: Vec<f64>,
}

#[derive(Clone, Debug)]
struct IndexedTrainingCorpus {
    feature_names: Vec<String>,
    examples: Vec<IndexedRankingExample>,
}

impl IndexedTrainingCorpus {
    fn compile(examples: &[RankingExample]) -> Self {
        let feature_names = examples
            .iter()
            .flat_map(|example| &example.candidates)
            .flat_map(|candidate| candidate.keys().map(String::as_str))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let feature_indices = feature_names
            .iter()
            .enumerate()
            .map(|(index, feature)| (feature.as_str(), index))
            .collect::<HashMap<_, _>>();
        let examples = examples
            .iter()
            .map(|example| IndexedRankingExample {
                target_probabilities: example.target_probabilities.clone(),
                neutral_indices: example.neutral_indices.clone(),
                base_logits: example.base_logits.clone(),
                candidates: example
                    .candidates
                    .iter()
                    .map(|candidate| {
                        candidate
                            .iter()
                            .map(|(feature, value)| {
                                (
                                    *feature_indices
                                        .get(feature.as_str())
                                        .expect("indexed feature must exist"),
                                    *value,
                                )
                            })
                            .collect()
                    })
                    .collect(),
            })
            .collect();
        Self {
            feature_names,
            examples,
        }
    }
}

#[derive(Clone, Copy)]
pub struct CombatActionImitationDemonstrationV1<'a> {
    pub root: &'a CombatPosition,
    pub actions: &'a [ClientInput],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatActionReanalysisEvidenceV1 {
    ExactWin { final_hp: i32 },
    ExactNonWin,
    BudgetUnknown,
}

/// Whether typed successor evidence distinguishes any action from the
/// exact-win support under the conservative v1 feasibility target. Exact-win
/// final HP remains available for a future quality target; v1 deliberately
/// does not convert it into an ordering. Consequently, an all-win surface
/// must not manufacture a v1 preference by copying the base distribution into
/// training.
pub fn combat_action_reanalysis_has_v1_preference_evidence(
    evidence: &[CombatActionReanalysisEvidenceV1],
) -> bool {
    evidence.iter().any(|evidence| {
        matches!(
            evidence,
            CombatActionReanalysisEvidenceV1::ExactNonWin
                | CombatActionReanalysisEvidenceV1::BudgetUnknown
        )
    })
}

#[derive(Clone, Debug)]
pub struct CombatActionReanalysisCandidateV1 {
    pub input: ClientInput,
    pub evidence: CombatActionReanalysisEvidenceV1,
}

#[derive(Clone, Copy, Debug)]
pub struct CombatActionReanalysisDecisionV1<'a> {
    pub root: &'a CombatPosition,
    pub candidates: &'a [CombatActionReanalysisCandidateV1],
}

/// One complete legal action surface with an externally constructed typed
/// probability target.  The target may encode heuristic boundary quality, but
/// it carries no terminal-win authority and cannot change simulator truth.
#[derive(Clone, Copy, Debug)]
pub struct CombatActionSoftTargetDecisionV1<'a> {
    pub root: &'a CombatPosition,
    pub candidates: &'a [ClientInput],
    pub target_probabilities: &'a [f64],
    pub top1_accepted_indices: &'a [usize],
}

#[derive(Clone, Copy, Debug)]
pub struct CombatActionReanalysisTrainingConfigV1 {
    /// Probability mass transferred to uniformly weighted exact-win support
    /// while budget-unknown alternatives remain. With no unknown alternatives
    /// there is no uncertainty mass to transfer: exact non-wins are removed
    /// and the base policy is renormalized over exact wins.
    pub exact_support_mass: f64,
}

impl Default for CombatActionReanalysisTrainingConfigV1 {
    fn default() -> Self {
        Self {
            exact_support_mass: 0.5,
        }
    }
}

/// Constructs a conservative policy-improvement target from typed bounded
/// evidence. `BudgetUnknown` candidates retain their relative base-policy
/// probability and therefore are not recast as losses.
pub fn conservative_combat_reanalysis_target_v1(
    base_weights: &[f64],
    evidence: &[CombatActionReanalysisEvidenceV1],
    config: CombatActionReanalysisTrainingConfigV1,
) -> Result<Vec<f64>, String> {
    if base_weights.is_empty() || base_weights.len() != evidence.len() {
        return Err("combat reanalysis target requires aligned non-empty inputs".to_string());
    }
    if !config.exact_support_mass.is_finite() || !(0.0..1.0).contains(&config.exact_support_mass) {
        return Err("combat reanalysis exact-support mass must be in 0..1".to_string());
    }
    let exact_wins = evidence
        .iter()
        .filter(|item| matches!(item, CombatActionReanalysisEvidenceV1::ExactWin { .. }))
        .count();
    if exact_wins == 0 {
        return Err("combat reanalysis target requires at least one exact win".to_string());
    }
    let unknown_count = evidence
        .iter()
        .filter(|item| matches!(item, CombatActionReanalysisEvidenceV1::BudgetUnknown))
        .count();

    let eligible = evidence
        .iter()
        .map(|item| !matches!(item, CombatActionReanalysisEvidenceV1::ExactNonWin))
        .collect::<Vec<_>>();
    let safe_weights = base_weights
        .iter()
        .map(|weight| {
            if weight.is_finite() && *weight > 0.0 {
                *weight
            } else {
                1.0
            }
        })
        .collect::<Vec<_>>();
    let max_weight = safe_weights
        .iter()
        .zip(&eligible)
        .filter(|(_, eligible)| **eligible)
        .map(|(weight, _)| *weight)
        .fold(f64::MIN_POSITIVE, f64::max);
    let scaled_total = safe_weights
        .iter()
        .zip(&eligible)
        .filter(|(_, eligible)| **eligible)
        .map(|(weight, _)| *weight / max_weight)
        .sum::<f64>()
        .max(f64::MIN_POSITIVE);
    let exact_support_mass = if unknown_count == 0 {
        0.0
    } else {
        config.exact_support_mass
    };
    let preserved_mass = 1.0 - exact_support_mass;
    Ok(safe_weights
        .iter()
        .zip(evidence)
        .zip(eligible)
        .map(|((weight, evidence), eligible)| {
            if !eligible {
                return 0.0;
            }
            let preserved = preserved_mass * (*weight / max_weight) / scaled_total;
            let exact_support =
                if matches!(evidence, CombatActionReanalysisEvidenceV1::ExactWin { .. }) {
                    exact_support_mass / exact_wins as f64
                } else {
                    0.0
                };
            preserved + exact_support
        })
        .collect())
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatActionImitationDecisionAuditV1 {
    pub action_index: usize,
    pub player_turn: u32,
    pub candidate_count: usize,
    pub demonstrated_rank: usize,
    pub demonstrated_input: ClientInput,
    pub demonstrated_action_key: String,
    pub best_input: ClientInput,
    pub best_action_key: String,
    pub demonstrated_logit: f64,
    pub best_logit: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatActionImitationAuditV1 {
    pub source_action_count: usize,
    pub ranked_decision_count: usize,
    pub skipped_forced_decision_count: usize,
    pub misses: Vec<CombatActionImitationDecisionAuditV1>,
}

/// Trains a cheap action policy from one exact, terminally verified combat
/// witness. The artifact contains typed state/action features and learned
/// coefficients only: no exact state hash, card lineage, hand index, or witness
/// action is available to the runtime policy.
pub fn train_combat_action_imitation_v1(
    root: &CombatPosition,
    demonstrated_actions: &[ClientInput],
    config: CombatActionImitationTrainingConfigV1,
) -> Result<CombatActionImitationArtifactV1, String> {
    train_combat_action_imitation_from_demonstrations_v1(
        &[CombatActionImitationDemonstrationV1 {
            root,
            actions: demonstrated_actions,
        }],
        config,
    )
}

pub fn train_combat_action_imitation_from_demonstrations_v1(
    demonstrations: &[CombatActionImitationDemonstrationV1<'_>],
    config: CombatActionImitationTrainingConfigV1,
) -> Result<CombatActionImitationArtifactV1, String> {
    train_combat_action_imitation_from_demonstrations_with_base_v1(
        demonstrations,
        config,
        Arc::new(UniformCombatActionPolicy),
    )
}

pub fn train_combat_action_imitation_from_demonstrations_with_base_v1(
    demonstrations: &[CombatActionImitationDemonstrationV1<'_>],
    config: CombatActionImitationTrainingConfigV1,
    base_policy: SharedCombatActionPolicy,
) -> Result<CombatActionImitationArtifactV1, String> {
    train_combat_action_imitation_with_reanalysis_and_base_v1(
        demonstrations,
        &[],
        config,
        CombatActionReanalysisTrainingConfigV1::default(),
        base_policy,
    )
}

pub fn train_combat_action_imitation_with_reanalysis_and_base_v1(
    demonstrations: &[CombatActionImitationDemonstrationV1<'_>],
    reanalysis: &[CombatActionReanalysisDecisionV1<'_>],
    config: CombatActionImitationTrainingConfigV1,
    reanalysis_config: CombatActionReanalysisTrainingConfigV1,
    base_policy: SharedCombatActionPolicy,
) -> Result<CombatActionImitationArtifactV1, String> {
    train_combat_action_imitation_with_soft_targets_and_base_v1(
        demonstrations,
        reanalysis,
        &[],
        config,
        reanalysis_config,
        base_policy,
    )
}

pub fn train_combat_action_imitation_with_soft_targets_and_base_v1(
    demonstrations: &[CombatActionImitationDemonstrationV1<'_>],
    reanalysis: &[CombatActionReanalysisDecisionV1<'_>],
    soft_targets: &[CombatActionSoftTargetDecisionV1<'_>],
    config: CombatActionImitationTrainingConfigV1,
    reanalysis_config: CombatActionReanalysisTrainingConfigV1,
    base_policy: SharedCombatActionPolicy,
) -> Result<CombatActionImitationArtifactV1, String> {
    train_combat_action_imitation_with_soft_targets_and_initial_v1(
        demonstrations,
        reanalysis,
        soft_targets,
        config,
        reanalysis_config,
        base_policy,
        None,
    )
}

pub fn train_combat_action_imitation_with_soft_targets_and_initial_v1(
    demonstrations: &[CombatActionImitationDemonstrationV1<'_>],
    reanalysis: &[CombatActionReanalysisDecisionV1<'_>],
    soft_targets: &[CombatActionSoftTargetDecisionV1<'_>],
    config: CombatActionImitationTrainingConfigV1,
    reanalysis_config: CombatActionReanalysisTrainingConfigV1,
    base_policy: SharedCombatActionPolicy,
    initial_artifact: Option<&CombatActionImitationArtifactV1>,
) -> Result<CombatActionImitationArtifactV1, String> {
    validate_training_config(config)?;
    if let Some(initial) = initial_artifact {
        initial.validate()?;
        if (initial.logit_scale - config.logit_scale).abs() > f64::EPSILON
            || (initial.max_abs_log_factor - config.max_abs_log_factor).abs() > f64::EPSILON
            || (initial.base_weight_exponent - config.base_weight_exponent).abs() > f64::EPSILON
        {
            return Err(
                "combat action imitation warm start uses a different runtime policy contract"
                    .to_string(),
            );
        }
    }
    if demonstrations.is_empty() {
        return Err(
            "combat action imitation requires at least one exact terminal demonstration"
                .to_string(),
        );
    }
    let stepper = EngineCombatStepper;
    let mut examples = Vec::new();
    let mut skipped_forced_decision_count = 0usize;
    let mut pairwise_comparison_count = 0usize;
    let mut source_action_count = 0usize;
    let mut source_terminal_final_hp = i32::MAX;
    let mut replacement_root_hashes = BTreeSet::new();
    for (source_index, decision) in reanalysis.iter().enumerate() {
        let hash = combat_exact_state_hash_v2(&decision.root.engine, &decision.root.combat);
        if !replacement_root_hashes.insert(hash) {
            return Err(format!(
                "combat action reanalysis decision {source_index} duplicates an exact root"
            ));
        }
    }
    for (source_index, decision) in soft_targets.iter().enumerate() {
        let hash = combat_exact_state_hash_v2(&decision.root.engine, &decision.root.combat);
        if !replacement_root_hashes.insert(hash) {
            return Err(format!(
                "combat action soft-target decision {source_index} duplicates an evidence root"
            ));
        }
    }

    for (source_index, demonstration) in demonstrations.iter().enumerate() {
        let mut position = demonstration.root.clone();
        source_action_count = source_action_count.saturating_add(demonstration.actions.len());
        for (action_index, demonstrated) in demonstration.actions.iter().enumerate() {
            if !stepper.is_legal_action(&position, demonstrated) {
                return Err(format!(
                    "demonstration {source_index} action {action_index} is not legal at its exact replay state"
                ));
            }
            let candidates = concrete_combat_action_candidates_for_witness_v1(
                &position,
                demonstrated,
                config.max_structured_alternatives,
            );
            let demonstrated_index = candidates
                .iter()
                .position(|candidate| candidate == demonstrated)
                .ok_or_else(|| {
                    format!(
                        "demonstration {source_index} action {action_index} was absent from its legal surface"
                    )
                })?;
            if candidates.len() > 1 {
                let exact_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
                if initial_artifact.is_none() && !replacement_root_hashes.contains(&exact_hash) {
                    let accepted_indices = exact_witness_adjacent_accepted_indices_v1(
                        &stepper,
                        &position,
                        demonstration.actions,
                        action_index,
                        &candidates,
                        demonstrated_index,
                        config.max_engine_steps_per_transition,
                    );
                    pairwise_comparison_count = pairwise_comparison_count
                        .saturating_add(candidates.len().saturating_sub(accepted_indices.len()));
                    let state = typed_combat_feature_components_v1(&position);
                    let mut target_probabilities = vec![0.0; candidates.len()];
                    target_probabilities[demonstrated_index] = 1.0;
                    examples.push(RankingExample {
                        target_probabilities,
                        neutral_indices: accepted_indices
                            .iter()
                            .copied()
                            .filter(|index| *index != demonstrated_index)
                            .collect(),
                        top1_accepted_indices: accepted_indices,
                        base_logits: concrete_base_logits(
                            &position,
                            &candidates,
                            base_policy.as_ref(),
                            config.base_weight_exponent,
                        ),
                        candidates: candidates
                            .iter()
                            .map(|candidate| {
                                action_feature_vector_with_state(&position, candidate, &state)
                            })
                            .collect(),
                    });
                }
            } else {
                skipped_forced_decision_count = skipped_forced_decision_count.saturating_add(1);
            }

            let step = stepper.apply_to_stable(
                &position,
                demonstrated.clone(),
                CombatStepLimits {
                    max_engine_steps: config.max_engine_steps_per_transition,
                    deadline: None,
                },
            );
            if step.truncated || step.timed_out {
                return Err(format!(
                    "demonstration {source_index} action {action_index} did not reach a stable exact successor"
                ));
            }
            position = step.position;
        }

        if stepper.terminal(&position) != CombatTerminal::Win
            || position.combat.runtime.combat_smoked
        {
            return Err(format!(
                "combat action imitation demonstration {source_index} is not an exact terminal victory"
            ));
        }
        source_terminal_final_hp =
            source_terminal_final_hp.min(position.combat.entities.player.current_hp);
    }

    for (source_index, decision) in reanalysis.iter().enumerate() {
        if stepper.terminal(decision.root) != CombatTerminal::Unresolved {
            return Err(format!(
                "combat action reanalysis decision {source_index} is already terminal"
            ));
        }
        let legal_surface = stepper.legal_action_surface(decision.root);
        if !legal_surface.selection_families.is_empty() {
            return Err(format!(
                "combat action reanalysis decision {source_index} has a structured action family; v1 requires a complete atomic surface"
            ));
        }
        if decision.candidates.len() != legal_surface.atomic_actions.len()
            || legal_surface.atomic_actions.iter().any(|expected| {
                !decision
                    .candidates
                    .iter()
                    .any(|candidate| &candidate.input == expected)
            })
        {
            return Err(format!(
                "combat action reanalysis decision {source_index} does not cover its complete atomic surface"
            ));
        }
        let mut unique_inputs = Vec::with_capacity(decision.candidates.len());
        for candidate in decision.candidates {
            if !stepper.is_legal_action(decision.root, &candidate.input)
                || unique_inputs.contains(&candidate.input)
            {
                return Err(format!(
                    "combat action reanalysis decision {source_index} contains an invalid or duplicate action"
                ));
            }
            unique_inputs.push(candidate.input.clone());
        }
        if decision.candidates.len() <= 1 {
            skipped_forced_decision_count = skipped_forced_decision_count.saturating_add(1);
            continue;
        }

        let choices = unique_inputs
            .iter()
            .map(CombatPolicyChoice::Atomic)
            .collect::<Vec<_>>();
        let base_weights = base_policy.weights(decision.root, &choices);
        if base_weights.len() != decision.candidates.len() {
            return Err(format!(
                "combat action reanalysis decision {source_index} received a misaligned base policy"
            ));
        }
        let typed_evidence = decision
            .candidates
            .iter()
            .map(|candidate| candidate.evidence)
            .collect::<Vec<_>>();
        if !combat_action_reanalysis_has_v1_preference_evidence(&typed_evidence) {
            continue;
        }
        let target_probabilities = conservative_combat_reanalysis_target_v1(
            &base_weights,
            &typed_evidence,
            reanalysis_config,
        )?;
        let exact_win_indices = typed_evidence
            .iter()
            .enumerate()
            .filter_map(|(index, evidence)| {
                matches!(evidence, CombatActionReanalysisEvidenceV1::ExactWin { .. })
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let exact_non_win_count = typed_evidence
            .iter()
            .filter(|evidence| matches!(evidence, CombatActionReanalysisEvidenceV1::ExactNonWin))
            .count();
        pairwise_comparison_count = pairwise_comparison_count
            .saturating_add(exact_win_indices.len().saturating_mul(exact_non_win_count));
        source_action_count = source_action_count.saturating_add(decision.candidates.len());
        for evidence in typed_evidence {
            if let CombatActionReanalysisEvidenceV1::ExactWin { final_hp } = evidence {
                source_terminal_final_hp = source_terminal_final_hp.min(final_hp);
            }
        }
        let state = typed_combat_feature_components_v1(decision.root);
        examples.push(RankingExample {
            target_probabilities,
            neutral_indices: Vec::new(),
            top1_accepted_indices: exact_win_indices,
            base_logits: concrete_base_logits(
                decision.root,
                &unique_inputs,
                base_policy.as_ref(),
                config.base_weight_exponent,
            ),
            candidates: unique_inputs
                .iter()
                .map(|candidate| action_feature_vector_with_state(decision.root, candidate, &state))
                .collect(),
        });
    }

    for (source_index, decision) in soft_targets.iter().enumerate() {
        if stepper.terminal(decision.root) != CombatTerminal::Unresolved {
            return Err(format!(
                "combat action soft-target decision {source_index} is already terminal"
            ));
        }
        let legal_surface = stepper.legal_action_surface(decision.root);
        if !legal_surface.selection_families.is_empty() {
            return Err(format!(
                "combat action soft-target decision {source_index} has a structured action family; v1 requires a complete atomic surface"
            ));
        }
        if decision.candidates.len() != legal_surface.atomic_actions.len()
            || legal_surface.atomic_actions.iter().any(|expected| {
                !decision
                    .candidates
                    .iter()
                    .any(|candidate| candidate == expected)
            })
        {
            return Err(format!(
                "combat action soft-target decision {source_index} does not cover its complete atomic surface"
            ));
        }
        if decision.candidates.len() != decision.target_probabilities.len()
            || decision.candidates.is_empty()
            || decision
                .target_probabilities
                .iter()
                .any(|probability| !probability.is_finite() || *probability < 0.0)
        {
            return Err(format!(
                "combat action soft-target decision {source_index} has an invalid probability target"
            ));
        }
        let target_total = decision.target_probabilities.iter().sum::<f64>();
        if (target_total - 1.0).abs() > 1.0e-9 {
            return Err(format!(
                "combat action soft-target decision {source_index} target sums to {target_total}, not one"
            ));
        }
        let mut unique_inputs = Vec::with_capacity(decision.candidates.len());
        for candidate in decision.candidates {
            if !stepper.is_legal_action(decision.root, candidate)
                || unique_inputs.contains(candidate)
            {
                return Err(format!(
                    "combat action soft-target decision {source_index} contains an invalid or duplicate action"
                ));
            }
            unique_inputs.push(candidate.clone());
        }
        if decision.candidates.len() <= 1 {
            skipped_forced_decision_count = skipped_forced_decision_count.saturating_add(1);
            continue;
        }
        let best_target = decision
            .target_probabilities
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        if decision.top1_accepted_indices.is_empty()
            || decision.top1_accepted_indices.iter().any(|index| {
                *index >= decision.candidates.len()
                    || (decision.target_probabilities[*index] - best_target).abs() > 1.0e-12
            })
        {
            return Err(format!(
                "combat action soft-target decision {source_index} has invalid top1 support"
            ));
        }
        let choices = unique_inputs
            .iter()
            .map(CombatPolicyChoice::Atomic)
            .collect::<Vec<_>>();
        let base_weights = base_policy.weights(decision.root, &choices);
        if base_weights.len() != decision.candidates.len() {
            return Err(format!(
                "combat action soft-target decision {source_index} received a misaligned base policy"
            ));
        }
        pairwise_comparison_count = pairwise_comparison_count.saturating_add(
            decision
                .target_probabilities
                .iter()
                .enumerate()
                .map(|(left_index, left)| {
                    decision.target_probabilities[left_index + 1..]
                        .iter()
                        .filter(|right| (*left - **right).abs() > 1.0e-12)
                        .count()
                })
                .sum::<usize>(),
        );
        source_action_count = source_action_count.saturating_add(decision.candidates.len());
        let state = typed_combat_feature_components_v1(decision.root);
        examples.push(RankingExample {
            target_probabilities: decision.target_probabilities.to_vec(),
            neutral_indices: Vec::new(),
            top1_accepted_indices: decision.top1_accepted_indices.to_vec(),
            base_logits: concrete_base_logits(
                decision.root,
                &unique_inputs,
                base_policy.as_ref(),
                config.base_weight_exponent,
            ),
            candidates: unique_inputs
                .iter()
                .map(|candidate| action_feature_vector_with_state(decision.root, candidate, &state))
                .collect(),
        });
    }

    if examples.is_empty() {
        return Err("combat action imitation source contains no ranked decisions".to_string());
    }

    let weights = train_sparse_softmax_with_initial(
        &examples,
        config,
        initial_artifact.map(|artifact| artifact.coefficients.as_slice()),
    );
    let training_top1_correct = examples
        .iter()
        .filter(|example| {
            example
                .top1_accepted_indices
                .contains(&runtime_candidate_index(
                    &weights,
                    example,
                    config.logit_scale,
                    config.max_abs_log_factor,
                ))
        })
        .count();
    let coefficients = weights
        .into_iter()
        .filter(|(_, weight)| weight.abs() >= 1.0e-10)
        .map(|(feature, weight)| CombatActionImitationCoefficientV1 { feature, weight })
        .collect::<Vec<_>>();
    let artifact = CombatActionImitationArtifactV1 {
        schema_name: COMBAT_ACTION_IMITATION_SCHEMA_NAME.to_string(),
        schema_version: COMBAT_ACTION_IMITATION_SCHEMA_VERSION,
        feature_schema: COMBAT_ACTION_FEATURE_SCHEMA.to_string(),
        runtime_compatibility_id: COMBAT_ACTION_IMITATION_RUNTIME_ID.to_string(),
        training_authority: if !soft_targets.is_empty() {
            "exact_terminal_win_demonstrations_with_complete_typed_action_soft_targets".to_string()
        } else if reanalysis.is_empty() {
            "exact_terminal_win_demonstration_with_exact_adjacent_alternatives_excluded_from_negatives"
                .to_string()
        } else {
            format!(
                "exact_terminal_win_demonstrations_with_exact_state_targets_replaced_by_typed_bounded_reanalysis_with_{:.6}_exact_support_mass_and_budget_unknown_base_mass",
                reanalysis_config.exact_support_mass
            )
        },
        source_trajectory_count: demonstrations.len(),
        source_action_count,
        source_terminal_final_hp,
        ranked_decision_count: examples.len(),
        pairwise_comparison_count,
        skipped_forced_decision_count,
        training_top1_correct,
        training_top1_total: examples.len(),
        logit_scale: config.logit_scale,
        max_abs_log_factor: config.max_abs_log_factor,
        base_weight_exponent: config.base_weight_exponent,
        coefficients,
    };
    artifact.validate()?;
    Ok(artifact)
}

/// Replays one verified demonstration and exposes only decisions whose learned
/// winner is absent from the demonstrated-or-exactly-accepted set. This is a
/// training-representation diagnostic; it neither changes policy weights nor
/// grants the witness runtime authority.
pub fn audit_combat_action_imitation_v1(
    root: &CombatPosition,
    demonstrated_actions: &[ClientInput],
    artifact: &CombatActionImitationArtifactV1,
    base_policy: &dyn CombatActionPolicy,
    max_structured_alternatives: usize,
    max_engine_steps_per_transition: usize,
) -> Result<CombatActionImitationAuditV1, String> {
    artifact.validate()?;
    let coefficients = artifact
        .coefficients
        .iter()
        .map(|coefficient| (coefficient.feature.clone(), coefficient.weight))
        .collect::<HashMap<_, _>>();
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut misses = Vec::new();
    let mut ranked_decision_count = 0usize;
    let mut skipped_forced_decision_count = 0usize;
    for (action_index, demonstrated) in demonstrated_actions.iter().enumerate() {
        if !stepper.is_legal_action(&position, demonstrated) {
            return Err(format!(
                "action imitation audit action {action_index} is not legal at its exact replay state"
            ));
        }
        let candidates = concrete_combat_action_candidates_for_witness_v1(
            &position,
            demonstrated,
            max_structured_alternatives,
        );
        let demonstrated_index = candidates
            .iter()
            .position(|candidate| candidate == demonstrated)
            .ok_or_else(|| {
                format!(
                    "action imitation audit action {action_index} is absent from its candidates"
                )
            })?;
        if candidates.len() > 1 {
            ranked_decision_count = ranked_decision_count.saturating_add(1);
            let state = typed_combat_feature_components_v1(&position);
            let logits = candidates
                .iter()
                .map(|candidate| {
                    sparse_score(
                        &coefficients,
                        &action_feature_vector_with_state(&position, candidate, &state),
                    ) * artifact.logit_scale
                })
                .collect::<Vec<_>>();
            let base_logits = concrete_base_logits(
                &position,
                &candidates,
                base_policy,
                artifact.base_weight_exponent,
            );
            let logits =
                runtime_combined_logits(&logits, &base_logits, artifact.max_abs_log_factor);
            let demonstrated_logit = logits[demonstrated_index];
            let demonstrated_rank = 1 + logits
                .iter()
                .enumerate()
                .filter(|(candidate_index, candidate)| {
                    candidate.total_cmp(&demonstrated_logit).is_gt()
                        || (candidate.total_cmp(&demonstrated_logit).is_eq()
                            && *candidate_index < demonstrated_index)
                })
                .count();
            let best_index = logits
                .iter()
                .enumerate()
                .max_by(|(left_index, left), (right_index, right)| {
                    left.total_cmp(right)
                        .then_with(|| right_index.cmp(left_index))
                })
                .map(|(index, _)| index)
                .unwrap_or_default();
            let accepted_indices = exact_witness_adjacent_accepted_indices_v1(
                &stepper,
                &position,
                demonstrated_actions,
                action_index,
                &candidates,
                demonstrated_index,
                max_engine_steps_per_transition,
            );
            if !accepted_indices.contains(&best_index) {
                misses.push(CombatActionImitationDecisionAuditV1 {
                    action_index,
                    player_turn: position.combat.turn.turn_count,
                    candidate_count: candidates.len(),
                    demonstrated_rank,
                    demonstrated_input: demonstrated.clone(),
                    demonstrated_action_key: combat_action_key(&position.combat, demonstrated),
                    best_input: candidates[best_index].clone(),
                    best_action_key: combat_action_key(&position.combat, &candidates[best_index]),
                    demonstrated_logit,
                    best_logit: logits[best_index],
                });
            }
        } else {
            skipped_forced_decision_count = skipped_forced_decision_count.saturating_add(1);
        }
        let step = stepper.apply_to_stable(
            &position,
            demonstrated.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "action imitation audit action {action_index} did not reach a stable exact successor"
            ));
        }
        position = step.position;
    }
    if stepper.terminal(&position) != CombatTerminal::Win || position.combat.runtime.combat_smoked {
        return Err("action imitation audit source is not an exact terminal victory".to_string());
    }
    Ok(CombatActionImitationAuditV1 {
        source_action_count: demonstrated_actions.len(),
        ranked_decision_count,
        skipped_forced_decision_count,
        misses,
    })
}

pub fn combat_action_imitation_policy_v1(
    base: SharedCombatActionPolicy,
    artifact: CombatActionImitationArtifactV1,
) -> Result<SharedCombatActionPolicy, String> {
    artifact.validate()?;
    let coefficients = CompiledActionImitationWeightsV1::new(&artifact.coefficients);
    Ok(Arc::new(CombatActionImitationPolicyV1 {
        base,
        coefficients,
        logit_scale: artifact.logit_scale,
        max_abs_log_factor: artifact.max_abs_log_factor,
        base_weight_exponent: artifact.base_weight_exponent,
    }))
}

/// Applies a specialized proposal policy only while constructing the current
/// root player turn.  Future turns return to the durable continuation policy;
/// their ordering belongs to cross-turn boundary guidance rather than to one
/// demonstrated action trace.
pub fn root_player_turn_action_policy_v1(
    root_player_turn: u32,
    root: SharedCombatActionPolicy,
    continuation: SharedCombatActionPolicy,
) -> SharedCombatActionPolicy {
    Arc::new(RootPlayerTurnActionPolicyV1 {
        root_player_turn,
        root,
        continuation,
    })
}

struct RootPlayerTurnActionPolicyV1 {
    root_player_turn: u32,
    root: SharedCombatActionPolicy,
    continuation: SharedCombatActionPolicy,
}

impl CombatActionPolicy for RootPlayerTurnActionPolicyV1 {
    fn weights(&self, position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        if position.combat.turn.turn_count == self.root_player_turn {
            self.root.weights(position, choices)
        } else {
            self.continuation.weights(position, choices)
        }
    }

    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
    ) -> sts_combat_planner::CombatSelectionPreferences {
        if position.combat.turn.turn_count == self.root_player_turn {
            self.root.structured_selection_preferences(position, family)
        } else {
            self.continuation
                .structured_selection_preferences(position, family)
        }
    }

    fn structured_selection_member_weights(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        if position.combat.turn.turn_count == self.root_player_turn {
            self.root
                .structured_selection_member_weights(position, family, members)
        } else {
            self.continuation
                .structured_selection_member_weights(position, family, members)
        }
    }

    fn state_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.continuation.state_guides(position)
    }

    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.continuation.turn_generation_guides(position)
    }
}

#[derive(Clone)]
struct CombatActionImitationPolicyV1 {
    base: SharedCombatActionPolicy,
    coefficients: CompiledActionImitationWeightsV1,
    logit_scale: f64,
    max_abs_log_factor: f64,
    base_weight_exponent: f64,
}

impl CombatActionImitationPolicyV1 {
    fn learned_logit(&self, position: &CombatPosition, input: &ClientInput, state: &[i32]) -> f64 {
        self.coefficients.score(position, input, state) * self.logit_scale
    }
}

/// The serialized artifact deliberately uses stable, inspectable feature
/// names. Rebuilding those names and a `BTreeMap` for every action expansion
/// is far too expensive for the search hot path, so loading compiles the same
/// linear model into token-local indexed dot products.
#[derive(Clone, Debug, Default)]
struct CompiledActionImitationWeightsV1 {
    action_by_token: HashMap<String, f64>,
    cross_by_token: HashMap<String, Vec<(usize, f64)>>,
    numeric: HashMap<String, f64>,
}

impl CompiledActionImitationWeightsV1 {
    fn new(coefficients: &[CombatActionImitationCoefficientV1]) -> Self {
        let mut compiled = Self::default();
        for coefficient in coefficients {
            if let Some(token) = coefficient.feature.strip_prefix("action/") {
                compiled
                    .action_by_token
                    .insert(token.to_string(), coefficient.weight);
                continue;
            }
            if let Some(cross) = coefficient.feature.strip_prefix("cross/") {
                if let Some((token, state_index)) = cross.rsplit_once("/state/") {
                    if let Ok(state_index) = state_index.parse::<usize>() {
                        compiled
                            .cross_by_token
                            .entry(token.to_string())
                            .or_default()
                            .push((state_index, coefficient.weight));
                        continue;
                    }
                }
            }
            compiled
                .numeric
                .insert(coefficient.feature.clone(), coefficient.weight);
        }
        compiled
    }

    fn score(&self, position: &CombatPosition, input: &ClientInput, state: &[i32]) -> f64 {
        let tokens = action_semantic_tokens(position, input);
        let token_scale = 1.0 / (tokens.len().max(1) as f64).sqrt();
        let mut score = 0.0;
        for token in tokens {
            score += self
                .action_by_token
                .get(&token)
                .copied()
                .unwrap_or_default()
                * token_scale;
            if let Some(cross) = self.cross_by_token.get(&token) {
                score += cross
                    .iter()
                    .map(|(index, weight)| {
                        state
                            .get(*index)
                            .copied()
                            .map(squash_component)
                            .unwrap_or_default()
                            * weight
                            * token_scale
                    })
                    .sum::<f64>();
            }
        }

        // Numeric action features are few (at most the played card and its
        // target). Keeping their stable names here avoids a second artifact
        // schema while removing the much larger token × state allocation.
        let mut numeric = SparseFeatures::new();
        add_numeric_action_features(position, input, &mut numeric);
        score + sparse_score(&self.numeric, &numeric)
    }
}

impl CombatActionPolicy for CombatActionImitationPolicyV1 {
    fn weights(&self, position: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        let base = self.base.weights(position, choices);
        let base = if base.len() == choices.len() {
            base
        } else {
            vec![1.0; choices.len()]
        };
        let state = typed_combat_feature_components_v1(position);
        let logits = choices
            .iter()
            .map(|choice| match choice {
                CombatPolicyChoice::Atomic(input) => {
                    Some(self.learned_logit(position, input, &state))
                }
                CombatPolicyChoice::StructuredSelection(_) => None,
            })
            .collect::<Vec<_>>();
        let atomic_logits = logits.iter().flatten().copied().collect::<Vec<_>>();
        let atomic_factors = normalized_learned_factors(&atomic_logits, self.max_abs_log_factor);
        let mut atomic_factor_index = 0usize;
        choices
            .iter()
            .zip(base)
            .zip(logits)
            .map(|((choice, base), logit)| match choice {
                CombatPolicyChoice::Atomic(_) => {
                    debug_assert!(logit.is_some());
                    let factor = atomic_factors[atomic_factor_index];
                    atomic_factor_index += 1;
                    positive_or_neutral(base).powf(self.base_weight_exponent) * factor
                }
                CombatPolicyChoice::StructuredSelection(_) => {
                    positive_or_neutral(base).powf(self.base_weight_exponent)
                }
            })
            .collect()
    }

    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
    ) -> sts_combat_planner::CombatSelectionPreferences {
        self.base.structured_selection_preferences(position, family)
    }

    fn structured_selection_member_weights(
        &self,
        position: &CombatPosition,
        family: &CombatSelectionActionFamilyV2,
        members: &[ClientInput],
    ) -> Vec<f64> {
        let base = self
            .base
            .structured_selection_member_weights(position, family, members);
        let base = if base.len() == members.len() {
            base
        } else {
            vec![1.0; members.len()]
        };
        let state = typed_combat_feature_components_v1(position);
        let logits = members
            .iter()
            .map(|member| self.learned_logit(position, member, &state))
            .collect::<Vec<_>>();
        let factors = normalized_learned_factors(&logits, self.max_abs_log_factor);
        members
            .iter()
            .zip(base)
            .zip(factors)
            .map(|((_member, base), factor)| {
                positive_or_neutral(base).powf(self.base_weight_exponent) * factor
            })
            .collect()
    }

    fn state_guide_rank(&self, position: &CombatPosition) -> Option<CombatStateGuideRank> {
        self.base.state_guide_rank(position)
    }

    fn state_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.base.state_guides(position)
    }

    fn turn_generation_guides(&self, position: &CombatPosition) -> Vec<CombatStateGuide> {
        self.base.turn_generation_guides(position)
    }
}

fn normalized_learned_factors(logits: &[f64], max_log_penalty: f64) -> Vec<f64> {
    let max_logit = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !max_logit.is_finite() {
        return vec![1.0; logits.len()];
    }
    logits
        .iter()
        .map(|logit| (logit - max_logit).clamp(-max_log_penalty, 0.0).exp())
        .collect()
}

fn validate_training_config(config: CombatActionImitationTrainingConfigV1) -> Result<(), String> {
    if config.epochs == 0
        || config.max_structured_alternatives == 0
        || config.max_engine_steps_per_transition == 0
        || !config.learning_rate.is_finite()
        || config.learning_rate <= 0.0
        || !config.l2_penalty.is_finite()
        || config.l2_penalty < 0.0
        || !config.logit_scale.is_finite()
        || config.logit_scale <= 0.0
        || !config.max_abs_log_factor.is_finite()
        || config.max_abs_log_factor <= 0.0
        || !config.base_weight_exponent.is_finite()
        || !(0.0..=1.0).contains(&config.base_weight_exponent)
    {
        return Err("invalid combat action imitation training configuration".to_string());
    }
    Ok(())
}

/// Materializes the bounded legal surface used to compare one action from an
/// exact terminal witness. The demonstrated action is retained even when it
/// belongs to a structured family beyond the materialization limit.
pub fn concrete_combat_action_candidates_for_witness_v1(
    position: &CombatPosition,
    demonstrated: &ClientInput,
    max_structured_alternatives: usize,
) -> Vec<ClientInput> {
    let mut candidates =
        concrete_combat_action_candidates_v1(position, max_structured_alternatives);
    if !candidates.contains(demonstrated) {
        candidates.push(demonstrated.clone());
    }
    let stepper = EngineCombatStepper;
    let mut unique = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if stepper.is_legal_action(position, &candidate) && !unique.contains(&candidate) {
            unique.push(candidate);
        }
    }
    unique
}

/// Materializes the same bounded legal-input surface used by semantic action
/// imitation. Atomic actions are complete. Canonical pending-choice families
/// are expanded lazily up to the explicit caller-provided limit.
pub fn concrete_combat_action_candidates_v1(
    position: &CombatPosition,
    max_structured_alternatives: usize,
) -> Vec<ClientInput> {
    let stepper = EngineCombatStepper;
    let mut candidates = stepper.atomic_actions(position);
    if let EngineState::PendingChoice(choice) = &position.engine {
        if let Some(inputs) =
            crate::ai::combat_search_v2::pending_choice_action_prefix::canonical_pending_choice_inputs(
                choice,
            )
        {
            candidates.extend(inputs.take(max_structured_alternatives));
        }
    }
    let mut unique = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if stepper.is_legal_action(position, &candidate) && !unique.contains(&candidate) {
            unique.push(candidate);
        }
    }
    unique
}

/// Returns actions that are already proven compatible with the same exact
/// terminal witness by swapping only the demonstrated action with its direct
/// successor. This is deliberately narrow: broader equivalence belongs to
/// bounded exact successor reanalysis, not imitation labels.
pub fn exact_witness_adjacent_accepted_indices_v1(
    stepper: &EngineCombatStepper,
    position: &CombatPosition,
    demonstrated_actions: &[ClientInput],
    action_index: usize,
    candidates: &[ClientInput],
    demonstrated_index: usize,
    max_engine_steps_per_transition: usize,
) -> Vec<usize> {
    let mut accepted_indices = vec![demonstrated_index];
    let limits = CombatStepLimits {
        max_engine_steps: max_engine_steps_per_transition,
        deadline: None,
    };
    let demonstrated = &demonstrated_actions[action_index];
    let Some(next_demonstrated) = demonstrated_actions.get(action_index + 1) else {
        accepted_indices.sort_unstable();
        accepted_indices.dedup();
        return accepted_indices;
    };
    let Some(after_demonstrated) = exact_stable_successor(stepper, position, demonstrated, limits)
    else {
        return accepted_indices;
    };
    let Some(swapped_first) =
        remap_input_by_card_lineage(&after_demonstrated, next_demonstrated, position)
    else {
        return accepted_indices;
    };
    let Some(swapped_index) = candidates
        .iter()
        .position(|candidate| candidate == &swapped_first)
    else {
        return accepted_indices;
    };
    if swapped_index == demonstrated_index {
        return accepted_indices;
    }
    let Some(after_swapped_first) =
        exact_stable_successor(stepper, position, &swapped_first, limits)
    else {
        return accepted_indices;
    };
    let Some(swapped_second) =
        remap_input_by_card_lineage(position, demonstrated, &after_swapped_first)
    else {
        return accepted_indices;
    };
    let Some(mut swapped_position) =
        exact_stable_successor(stepper, &after_swapped_first, &swapped_second, limits)
    else {
        return accepted_indices;
    };
    for suffix_action in &demonstrated_actions[action_index.saturating_add(2)..] {
        let Some(successor) =
            exact_stable_successor(stepper, &swapped_position, suffix_action, limits)
        else {
            return accepted_indices;
        };
        swapped_position = successor;
    }
    if stepper.terminal(&swapped_position) == CombatTerminal::Win
        && !swapped_position.combat.runtime.combat_smoked
    {
        accepted_indices.push(swapped_index);
    }
    accepted_indices.sort_unstable();
    accepted_indices.dedup();
    accepted_indices
}

fn exact_stable_successor(
    stepper: &EngineCombatStepper,
    position: &CombatPosition,
    input: &ClientInput,
    limits: CombatStepLimits,
) -> Option<CombatPosition> {
    if !stepper.is_legal_action(position, input) {
        return None;
    }
    let step = stepper.apply_to_stable(position, input.clone(), limits);
    (!step.truncated && !step.timed_out).then_some(step.position)
}

fn remap_input_by_card_lineage(
    source: &CombatPosition,
    input: &ClientInput,
    destination: &CombatPosition,
) -> Option<ClientInput> {
    match input {
        ClientInput::PlayCard { card_index, target } => {
            let lineage = source.combat.zones.hand.get(*card_index)?.lineage;
            let card_index = destination
                .combat
                .zones
                .hand
                .iter()
                .position(|card| card.lineage == lineage)?;
            Some(ClientInput::PlayCard {
                card_index,
                target: *target,
            })
        }
        _ => Some(input.clone()),
    }
}

fn concrete_base_logits(
    position: &CombatPosition,
    candidates: &[ClientInput],
    base_policy: &dyn CombatActionPolicy,
    exponent: f64,
) -> Vec<f64> {
    if exponent <= 0.0 {
        return vec![0.0; candidates.len()];
    }
    let choices = candidates
        .iter()
        .map(CombatPolicyChoice::Atomic)
        .collect::<Vec<_>>();
    let weights = base_policy.weights(position, &choices);
    if weights.len() != candidates.len() {
        return vec![0.0; candidates.len()];
    }
    weights
        .into_iter()
        .map(|weight| positive_or_neutral(weight).ln() * exponent)
        .collect()
}

#[cfg(test)]
fn train_sparse_softmax(
    examples: &[RankingExample],
    config: CombatActionImitationTrainingConfigV1,
) -> BTreeMap<String, f64> {
    train_sparse_softmax_with_initial(examples, config, None)
}

fn train_sparse_softmax_with_initial(
    examples: &[RankingExample],
    config: CombatActionImitationTrainingConfigV1,
    initial_coefficients: Option<&[CombatActionImitationCoefficientV1]>,
) -> BTreeMap<String, f64> {
    let corpus = IndexedTrainingCorpus::compile(examples);
    let initial_by_feature = initial_coefficients
        .unwrap_or_default()
        .iter()
        .map(|coefficient| (coefficient.feature.clone(), coefficient.weight))
        .collect::<BTreeMap<_, _>>();
    let mut weights = vec![0.0; corpus.feature_names.len()];
    for (index, feature) in corpus.feature_names.iter().enumerate() {
        if let Some(initial) = initial_by_feature.get(feature) {
            weights[index] = *initial;
        }
    }
    let regularization_center = weights.clone();
    for epoch in 0..config.epochs {
        let learning_rate = config.learning_rate / (1.0 + epoch as f64 * 0.05).sqrt();
        let shrink = (1.0 - learning_rate * config.l2_penalty).clamp(0.0, 1.0);
        for (weight, center) in weights.iter_mut().zip(&regularization_center) {
            *weight = *center + (*weight - *center) * shrink;
        }
        for example in &corpus.examples {
            let learned_logits = example
                .candidates
                .iter()
                .map(|candidate| indexed_sparse_score(&weights, candidate) * config.logit_scale)
                .collect::<Vec<_>>();
            // Match runtime clipping in the forward pass. The update below is
            // deliberately a straight-through gradient so a target currently
            // below the residual floor can still escape that floor.
            let scores = runtime_combined_logits(
                &learned_logits,
                &example.base_logits,
                config.max_abs_log_factor,
            );
            let is_active = |candidate_index: usize| {
                example.target_probabilities[candidate_index] > 0.0
                    || !example.neutral_indices.contains(&candidate_index)
            };
            let max_score = scores
                .iter()
                .enumerate()
                .filter(|(candidate_index, _)| is_active(*candidate_index))
                .map(|(_, score)| *score)
                .fold(f64::NEG_INFINITY, f64::max);
            let exponentials = scores
                .iter()
                .enumerate()
                .map(|(candidate_index, score)| {
                    if is_active(candidate_index) {
                        (score - max_score).exp()
                    } else {
                        0.0
                    }
                })
                .collect::<Vec<_>>();
            let total = exponentials.iter().sum::<f64>().max(f64::MIN_POSITIVE);
            for (candidate_index, candidate) in example.candidates.iter().enumerate() {
                if !is_active(candidate_index) {
                    continue;
                }
                let target = example.target_probabilities[candidate_index];
                let gradient =
                    (target - exponentials[candidate_index] / total) * config.logit_scale;
                if gradient.abs() < f64::EPSILON {
                    continue;
                }
                for &(feature_index, value) in candidate {
                    weights[feature_index] += learning_rate * gradient * value;
                }
            }
        }
    }
    let mut learned = corpus
        .feature_names
        .into_iter()
        .zip(weights)
        .collect::<BTreeMap<_, _>>();
    for (feature, weight) in initial_by_feature {
        learned.entry(feature).or_insert(weight);
    }
    learned
}

fn indexed_sparse_score(weights: &[f64], features: &[(usize, f64)]) -> f64 {
    features
        .iter()
        .map(|(feature_index, value)| weights[*feature_index] * value)
        .sum()
}

#[cfg(test)]
fn train_sparse_softmax_reference(
    examples: &[RankingExample],
    config: CombatActionImitationTrainingConfigV1,
) -> BTreeMap<String, f64> {
    let mut weights = BTreeMap::<String, f64>::new();
    for epoch in 0..config.epochs {
        let learning_rate = config.learning_rate / (1.0 + epoch as f64 * 0.05).sqrt();
        let shrink = (1.0 - learning_rate * config.l2_penalty).clamp(0.0, 1.0);
        for weight in weights.values_mut() {
            *weight *= shrink;
        }
        for example in examples {
            let learned_logits = example
                .candidates
                .iter()
                .map(|candidate| sparse_score(&weights, candidate) * config.logit_scale)
                .collect::<Vec<_>>();
            let scores = runtime_combined_logits(
                &learned_logits,
                &example.base_logits,
                config.max_abs_log_factor,
            );
            let is_active = |candidate_index: usize| {
                example.target_probabilities[candidate_index] > 0.0
                    || !example.neutral_indices.contains(&candidate_index)
            };
            let max_score = scores
                .iter()
                .enumerate()
                .filter(|(candidate_index, _)| is_active(*candidate_index))
                .map(|(_, score)| *score)
                .fold(f64::NEG_INFINITY, f64::max);
            let exponentials = scores
                .iter()
                .enumerate()
                .map(|(candidate_index, score)| {
                    if is_active(candidate_index) {
                        (score - max_score).exp()
                    } else {
                        0.0
                    }
                })
                .collect::<Vec<_>>();
            let total = exponentials.iter().sum::<f64>().max(f64::MIN_POSITIVE);
            for (candidate_index, candidate) in example.candidates.iter().enumerate() {
                if !is_active(candidate_index) {
                    continue;
                }
                let target = example.target_probabilities[candidate_index];
                let gradient =
                    (target - exponentials[candidate_index] / total) * config.logit_scale;
                if gradient.abs() < f64::EPSILON {
                    continue;
                }
                for (feature, value) in candidate {
                    *weights.entry(feature.clone()).or_default() +=
                        learning_rate * gradient * value;
                }
            }
        }
    }
    weights
}

fn runtime_candidate_index(
    weights: &BTreeMap<String, f64>,
    example: &RankingExample,
    logit_scale: f64,
    max_abs_log_factor: f64,
) -> usize {
    let learned = example
        .candidates
        .iter()
        .map(|candidate| sparse_score(weights, candidate) * logit_scale)
        .collect::<Vec<_>>();
    runtime_combined_logits(&learned, &example.base_logits, max_abs_log_factor)
        .iter()
        .enumerate()
        .max_by(|(left_index, left), (right_index, right)| {
            left.total_cmp(right)
                .then_with(|| right_index.cmp(left_index))
        })
        .map(|(index, _)| index)
        .unwrap_or_default()
}

fn runtime_combined_logits(
    learned_logits: &[f64],
    base_logits: &[f64],
    max_abs_log_factor: f64,
) -> Vec<f64> {
    debug_assert_eq!(learned_logits.len(), base_logits.len());
    let max_learned = learned_logits
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    learned_logits
        .iter()
        .zip(base_logits)
        .map(|(learned, base)| {
            let residual = if max_learned.is_finite() {
                (learned - max_learned).clamp(-max_abs_log_factor, 0.0)
            } else {
                0.0
            };
            base + residual
        })
        .collect()
}

fn sparse_score<W>(weights: &W, features: &SparseFeatures) -> f64
where
    W: SparseWeightLookup,
{
    features
        .iter()
        .map(|(feature, value)| weights.weight(feature) * value)
        .sum()
}

trait SparseWeightLookup {
    fn weight(&self, feature: &str) -> f64;
}

impl SparseWeightLookup for BTreeMap<String, f64> {
    fn weight(&self, feature: &str) -> f64 {
        self.get(feature).copied().unwrap_or_default()
    }
}

impl SparseWeightLookup for HashMap<String, f64> {
    fn weight(&self, feature: &str) -> f64 {
        self.get(feature).copied().unwrap_or_default()
    }
}

fn positive_or_neutral(weight: f64) -> f64 {
    if weight.is_finite() && weight > 0.0 {
        weight
    } else {
        1.0
    }
}

#[cfg(test)]
fn action_feature_vector(position: &CombatPosition, input: &ClientInput) -> SparseFeatures {
    let state = typed_combat_feature_components_v1(position);
    action_feature_vector_with_state(position, input, &state)
}

fn action_feature_vector_with_state(
    position: &CombatPosition,
    input: &ClientInput,
    state: &[i32],
) -> SparseFeatures {
    let mut features = SparseFeatures::new();
    let tokens = action_semantic_tokens(position, input);
    let token_scale = 1.0 / (tokens.len().max(1) as f64).sqrt();
    for token in tokens {
        add_feature(&mut features, format!("action/{token}"), token_scale);
        for (index, component) in state.iter().copied().enumerate() {
            add_feature(
                &mut features,
                format!("cross/{token}/state/{index}"),
                token_scale * squash_component(component),
            );
        }
    }
    add_numeric_action_features(position, input, &mut features);
    features
}

fn action_semantic_tokens(position: &CombatPosition, input: &ClientInput) -> Vec<String> {
    let mut tokens = Vec::new();
    match input {
        ClientInput::PlayCard { card_index, target } => {
            tokens.push("kind/play_card".to_string());
            let mut card_tokens = Vec::new();
            if let Some(card) = position.combat.zones.hand.get(*card_index) {
                let definition = get_card_definition(card.id);
                card_tokens.push(format!("card/{}+{}", card.id.stable_id(), card.upgrades));
                card_tokens.push(format!(
                    "card_type/{}",
                    definition.card_type.vocabulary_token_v1()
                ));
                tokens.extend(card_tokens.iter().cloned());
                push_strategic_card_semantic_tokens(card.id, card.upgrades, &mut tokens);
            }
            let target_start = tokens.len();
            push_target_tokens(position, *target, &mut tokens);
            let target_tokens = tokens[target_start..].to_vec();
            for card_token in &card_tokens {
                for target_token in &target_tokens {
                    tokens.push(format!("interaction/{card_token}/{target_token}"));
                }
            }
        }
        ClientInput::UsePotion {
            potion_index,
            target,
        } => {
            tokens.push("kind/use_potion".to_string());
            if let Some(Some(potion)) = position.combat.entities.potions.get(*potion_index) {
                tokens.push(format!("potion/{}", potion.id.stable_id()));
            }
            push_target_tokens(position, *target, &mut tokens);
        }
        ClientInput::DiscardPotion(_) => tokens.push("kind/discard_potion".to_string()),
        ClientInput::EndTurn => tokens.push("kind/end_turn".to_string()),
        ClientInput::SubmitSelection(resolution) => {
            tokens.push(format!(
                "kind/selection/{}",
                resolution.scope.vocabulary_token_v1()
            ));
            for lineage in resolution.selected_card_lineages() {
                if let Some(card) = combat_card_by_lineage(position, lineage) {
                    tokens.push(format!(
                        "selected_card/{}+{}",
                        card.id.stable_id(),
                        card.upgrades
                    ));
                } else {
                    tokens.push("selected_card/unknown".to_string());
                }
            }
        }
        ClientInput::SubmitScryDiscard(indices) => {
            tokens.push("kind/scry_discard".to_string());
            for index in indices {
                if let Some(card) = position.combat.zones.draw_pile.get(*index) {
                    tokens.push(format!(
                        "selected_card/{}+{}",
                        card.id.stable_id(),
                        card.upgrades
                    ));
                }
            }
        }
        ClientInput::SubmitDiscoverChoice(index) => {
            tokens.push("kind/discover_choice".to_string());
            push_discover_choice_tokens(position, *index, &mut tokens);
        }
        ClientInput::Cancel => tokens.push("kind/cancel".to_string()),
        ClientInput::Proceed => tokens.push("kind/proceed".to_string()),
        _ => tokens.push("kind/non_combat_input".to_string()),
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

fn push_strategic_card_semantic_tokens(card: CardId, upgrades: u8, tokens: &mut Vec<String>) {
    let semantics = strategic_card_definition(card, upgrades);
    for effect in semantics.play_effects {
        tokens.push(format!(
            "semantic/play_effect/{}",
            effect.vocabulary_token_v1()
        ));
        if let PlayEffect::Provide(mechanic) = effect {
            tokens.push(format!(
                "semantic/provides/{}",
                mechanic.vocabulary_token_v1()
            ));
            tokens.push(format!(
                "semantic/provides_immediately/{}",
                mechanic.vocabulary_token_v1()
            ));
        }
    }
    for rule in semantics.installed_rules {
        tokens.push(format!("semantic/installs/{}", rule.vocabulary_token_v1()));
    }
    for handler in semantics.event_handlers {
        tokens.push(format!(
            "semantic/event_handler/{}/{}",
            handler.on.vocabulary_token_v1(),
            handler.effect.vocabulary_token_v1()
        ));
        if let TriggeredEffect::Provide(mechanic) = handler.effect {
            tokens.push(format!(
                "semantic/provides/{}",
                mechanic.vocabulary_token_v1()
            ));
            tokens.push(format!(
                "semantic/provides_on/{}/{}",
                handler.on.vocabulary_token_v1(),
                mechanic.vocabulary_token_v1()
            ));
        }
    }
    for requirement in semantics.payoff_requirements {
        tokens.push(format!(
            "semantic/requires/{}",
            requirement.vocabulary_token_v1()
        ));
    }
    for burden in semantics.burdens {
        tokens.push(format!("semantic/burden/{}", burden.vocabulary_token_v1()));
    }
    for behavior in semantics.duplicate_behaviors {
        tokens.push(format!(
            "semantic/duplicate/{}",
            behavior.vocabulary_token_v1()
        ));
    }
}

fn push_target_tokens(
    position: &CombatPosition,
    target: Option<crate::EntityId>,
    tokens: &mut Vec<String>,
) {
    match target {
        None => tokens.push("target/none".to_string()),
        Some(entity) => {
            tokens.push("target/enemy".to_string());
            if let Some(monster) = position
                .combat
                .entities
                .monsters
                .iter()
                .find(|monster| monster.id == entity)
            {
                tokens.push(format!("target/slot/{}", monster.slot));
                tokens.push(format!("target/enemy/{}", monster.enemy_id.stable_id()));
                if monster.block > 0 {
                    tokens.push("target/has_block".to_string());
                }
                for power in [
                    PowerId::Artifact,
                    PowerId::Vulnerable,
                    PowerId::Weak,
                    PowerId::Strength,
                    PowerId::Flight,
                    PowerId::SharpHide,
                    PowerId::Malleable,
                    PowerId::Minion,
                ] {
                    let amount = position.combat.get_power(monster.id, power);
                    if amount != 0 {
                        tokens.push(format!(
                            "target/power/{}/{}",
                            power.stable_id(),
                            amount.signum()
                        ));
                    }
                }
            }
        }
    }
}

fn push_discover_choice_tokens(position: &CombatPosition, index: usize, tokens: &mut Vec<String>) {
    use crate::state::core::PendingChoice;

    let EngineState::PendingChoice(choice) = &position.engine else {
        return;
    };
    let selected = match choice {
        PendingChoice::DiscoverySelect(choice) => choice.cards.get(index).map(|card| (*card, 0)),
        PendingChoice::CardRewardSelect { cards, .. } => cards.get(index).map(|card| (*card, 0)),
        PendingChoice::ForeignInfluenceSelect { cards, upgraded } => cards
            .get(index)
            .map(|card| (*card, usize::from(*upgraded) as u8)),
        PendingChoice::ChooseOneSelect { choices } => choices
            .get(index)
            .map(|choice| (choice.card_id, choice.upgrades)),
        PendingChoice::StanceChoice => {
            tokens.push(format!("choice/stance/{index}"));
            None
        }
        _ => None,
    };
    if let Some((card, upgrades)) = selected {
        push_choice_card_tokens(card, upgrades, tokens);
    }
}

fn push_choice_card_tokens(card: CardId, upgrades: u8, tokens: &mut Vec<String>) {
    let definition = get_card_definition(card);
    tokens.push(format!("choice/card/{}+{upgrades}", card.stable_id()));
    tokens.push(format!(
        "choice/card_type/{}",
        definition.card_type.vocabulary_token_v1()
    ));
}

fn add_numeric_action_features(
    position: &CombatPosition,
    input: &ClientInput,
    features: &mut SparseFeatures,
) {
    match input {
        ClientInput::PlayCard { card_index, target } => {
            if let Some(card) = position.combat.zones.hand.get(*card_index) {
                let definition = get_card_definition(card.id);
                add_feature(
                    features,
                    "numeric/card/base_damage".to_string(),
                    squash_component(definition.base_damage),
                );
                add_feature(
                    features,
                    "numeric/card/base_block".to_string(),
                    squash_component(definition.base_block),
                );
                add_feature(
                    features,
                    "numeric/card/base_magic".to_string(),
                    squash_component(definition.base_magic),
                );
                let cost = card
                    .cost_for_turn
                    .map(i32::from)
                    .unwrap_or(i32::from(definition.cost) + i32::from(card.cost_modifier));
                add_feature(
                    features,
                    "numeric/card/cost".to_string(),
                    squash_component(cost),
                );
                add_feature(
                    features,
                    "numeric/card/exhaust".to_string(),
                    f64::from(card.exhaust_override.unwrap_or(definition.exhaust)),
                );
            }
            if let Some(monster) = target.and_then(|entity| {
                position
                    .combat
                    .entities
                    .monsters
                    .iter()
                    .find(|monster| monster.id == entity)
            }) {
                for (name, value) in [
                    ("current_hp", monster.current_hp),
                    ("max_hp", monster.max_hp),
                    ("block", monster.block),
                    (
                        "artifact",
                        position.combat.get_power(monster.id, PowerId::Artifact),
                    ),
                    (
                        "vulnerable",
                        position.combat.get_power(monster.id, PowerId::Vulnerable),
                    ),
                    ("weak", position.combat.get_power(monster.id, PowerId::Weak)),
                    (
                        "strength",
                        position.combat.get_power(monster.id, PowerId::Strength),
                    ),
                ] {
                    add_feature(
                        features,
                        format!("numeric/target/{name}"),
                        squash_component(value),
                    );
                }
            }
        }
        ClientInput::SubmitSelection(resolution) => add_feature(
            features,
            "numeric/selection/count".to_string(),
            squash_component(i32::try_from(resolution.selected.len()).unwrap_or(i32::MAX)),
        ),
        ClientInput::SubmitScryDiscard(indices) => add_feature(
            features,
            "numeric/selection/count".to_string(),
            squash_component(i32::try_from(indices.len()).unwrap_or(i32::MAX)),
        ),
        _ => {}
    }
}

fn combat_card_by_lineage(
    position: &CombatPosition,
    lineage: CardLineageId,
) -> Option<&crate::runtime::combat::CombatCard> {
    position
        .combat
        .zones
        .hand
        .iter()
        .chain(&position.combat.zones.draw_pile)
        .chain(&position.combat.zones.discard_pile)
        .chain(&position.combat.zones.exhaust_pile)
        .chain(&position.combat.zones.limbo)
        .find(|card| card.lineage == lineage)
}

fn add_feature(features: &mut SparseFeatures, name: String, value: f64) {
    if value.is_finite() && value.abs() >= 1.0e-12 {
        *features.entry(name).or_default() += value;
    }
}

fn squash_component(value: i32) -> f64 {
    (f64::from(value).asinh() / 8.0).clamp(-1.0, 1.0)
}

pub fn typed_combat_feature_components_v1(position: &CombatPosition) -> Vec<i32> {
    let mut features =
        crate::ai::combat_search_v2::oracle_action_policy::oracle_combat_state_guide_components(
            position,
        );
    features.extend(
        crate::ai::combat_search_v2::oracle_action_policy::oracle_combat_survival_guide_components(
            position,
        ),
    );
    features.extend(
        crate::ai::combat_search_v2::oracle_action_policy::oracle_combat_horizon_guide_components(
            position,
        ),
    );
    features.extend(
        crate::ai::combat_search_v2::oracle_action_policy::oracle_combat_setup_guide_components(
            position,
        ),
    );
    features.extend(
        crate::ai::combat_search_v2::oracle_action_policy::oracle_combat_turn_generation_guide_components(
            position,
        ),
    );
    features
}

#[cfg(test)]
mod tests;
