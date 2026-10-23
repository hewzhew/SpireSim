//! Algorithm-neutral public projection of exact complete-turn candidates.
//!
//! This crate does not enumerate candidates, score them, or assign outcomes.
//! It removes private resolution handles from one already materialized exact
//! proposal and retains only public observations, selected-action semantics,
//! resource provenance, and typed combat-plan transitions.  Training targets
//! and execution selection belong to downstream callers.

fn draft_message(
    locale: sts_presentation::Locale,
    key: &str,
    parameters: &[(&str, &str)],
) -> String {
    locale
        .text("draft", key, parameters)
        .expect("combat draft message")
}

mod combat_draft;
mod draft_successor;
pub use draft_successor::{
    render_combat_draft_successor_v1, CombatDraftMonsterSummaryV1, CombatDraftSuccessorSummaryV1,
};
mod selection_intent;
mod selection_outcomes;
pub use selection_outcomes::{
    project_combat_draft_selection_outcomes_v1, render_combat_draft_selection_outcomes_v1,
    CombatDraftSelectionOutcomeV1, CombatDraftSelectionOutcomesViewV1,
};

pub use selection_intent::{
    project_combat_draft_selection_at_v1, project_combat_draft_selection_context_v1,
    project_combat_draft_selection_surface_at_v1, project_combat_draft_selection_v1,
    reason_label as combat_selection_reason_label_v1, render_combat_draft_selection_v1,
    summarize_combat_draft_selection_source_v1, CombatDraftSelectionContextV1,
    CombatDraftSelectionMemberV1, CombatDraftSelectionViewV1,
};

pub use combat_draft::{
    project_combat_draft_option_v1, project_combat_draft_turn_v1,
    render_action_brief as render_combat_draft_action_v1, render_combat_draft_option_v1,
    render_combat_draft_turn_v1, CombatDraftOptionIndexEntryV1, CombatDraftOptionViewV1,
    CombatDraftTurnViewV1,
};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use sts_agent::agent::information::action::{
    project_public_combat_actions_v1, public_choice_for_exact_combat_input_v1,
    PublicCombatActionChoiceV1, PublicCombatActionSurfaceV1, PublicCombatAtomicActionV1,
    PublicCombatSelectionFamilyV1,
};
use sts_agent::agent::information::state::{
    public_combat_state_v1, CombatLearningCardV1, CombatLearningMonsterStateV1,
    CombatLearningPotionV1, PublicCombatStateV1,
};
use sts_combat_strategy::{
    combat_plan_projection_v1, combat_plan_transition_annotation_v1, CombatPlanProjectionV1,
    CombatPlanTransitionAnnotationV1,
};
use sts_complete_turn_proposal::{
    ExactCompleteTurnCompletedEndpointLayerV1, ExactCompleteTurnPrefixLayerV1,
    ExactCompleteTurnProposalBoundaryV1, ExactCompleteTurnProposalGuideRankV1,
    ExactCompleteTurnProposalSupportClaimV1, ExactCompleteTurnProposalV1,
};
use sts_core::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_core::ai::planner_core::stable_planner_id;
use sts_core::sim::combat::{
    combat_terminal, CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal,
    EngineCombatStepper,
};
use sts_core::state::core::ClientInput;

pub const COMPLETE_TURN_DECISION_VIEW_SCHEMA: &str = "complete_turn_decision_view_v1";
pub const COMPLETE_TURN_COMPLETED_ENDPOINT_LAYER_VIEW_SCHEMA: &str =
    "complete_turn_completed_endpoint_layer_view_v1";
pub const COMPLETE_TURN_PREFIX_LAYER_VIEW_SCHEMA: &str = "complete_turn_prefix_layer_view_v1";
pub const COMPLETE_TURN_WINNING_DEMONSTRATION_SCHEMA: &str =
    "complete_turn_winning_demonstration_v1";
pub const COMPLETE_TURN_POSITIVE_PREFIX_GUIDE_LANE_V1: u32 = 0x4354_5001;
pub const COMPLETE_TURN_CANDIDATE_ID_NAMESPACE_V1: &str = "run_play_combat_turn_candidate_v1";

/// One public-only state at an intermediate or completed point in a proposed
/// player turn. The action surface distinguishes public pending-choice states
/// that can share the same combat counters. Terminal kind is public outcome
/// evidence; exact state identity and RNG never enter this value.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnPublicPrefixStateV1 {
    pub observation: PublicCombatStateV1,
    pub boundary: CompleteTurnPublicPrefixBoundaryV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CompleteTurnPublicPrefixBoundaryV1 {
    Decision {
        action_surface: PublicCombatActionSurfaceV1,
    },
    TerminalWin,
    TerminalLoss,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteTurnPrefixGuideRankV1 {
    pub lane: u32,
    pub components: Vec<i32>,
}

/// Caller-owned guidance over a public prefix state. Returning no ranks means
/// unknown, not negative. Enumeration may reserve a representative for a
/// returned lane but cannot treat the score as legality or outcome evidence.
pub trait CompleteTurnPublicPrefixScorerV1: Send + Sync {
    fn supports_root(&self, _root: &CompleteTurnPublicPrefixStateV1) -> bool {
        true
    }

    fn guide_ranks(
        &self,
        state: &CompleteTurnPublicPrefixStateV1,
    ) -> Vec<CompleteTurnPrefixGuideRankV1>;
}

/// Same-root teacher evidence used to prove that public prefix guidance can
/// carry a replay-verified winning turn through a bounded sampler. This exact
/// matcher is deliberately not a learned or generalized policy.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnPositivePrefixEvidenceV1 {
    pub root: CompleteTurnPublicPrefixStateV1,
    pub verified_positive_prefixes: Vec<CompleteTurnPublicPrefixStateV1>,
}

impl CompleteTurnPositivePrefixEvidenceV1 {
    pub fn from_replayed_complete_turn(
        root: &CombatPosition,
        inputs: &[ClientInput],
        max_engine_steps_per_action: usize,
    ) -> Result<Self, String> {
        if inputs.is_empty() {
            return Err("positive complete-turn prefix evidence has no inputs".to_owned());
        }
        let stepper = EngineCombatStepper;
        let root_turn = root.combat.turn.turn_count;
        let public_root = project_complete_turn_public_prefix_state_v1(root)?;
        let mut position = root.clone();
        let mut verified_positive_prefixes = Vec::with_capacity(inputs.len());
        for (index, input) in inputs.iter().enumerate() {
            if !stepper.is_legal_action(&position, input)
                && stepper.choice_for_legal_input(&position, input).is_none()
            {
                return Err(format!(
                    "positive complete-turn prefix input {index} is not legal: {input:?}"
                ));
            }
            let result = stepper.apply_to_stable(
                &position,
                input.clone(),
                CombatStepLimits {
                    max_engine_steps: max_engine_steps_per_action.max(1),
                    deadline: None,
                },
            );
            if result.truncated || result.timed_out {
                return Err(format!(
                    "positive complete-turn prefix input {index} exceeded its transition limit"
                ));
            }
            position = result.position;
            let public = project_complete_turn_public_prefix_state_v1(&position)?;
            if !verified_positive_prefixes.contains(&public) {
                verified_positive_prefixes.push(public);
            }
        }
        let terminal = combat_terminal(&position.engine, &position.combat);
        if terminal == CombatTerminal::Unresolved && position.combat.turn.turn_count == root_turn {
            return Err(
                "positive prefix inputs do not reach a complete-turn or terminal boundary"
                    .to_owned(),
            );
        }
        Ok(Self {
            root: public_root,
            verified_positive_prefixes,
        })
    }

    pub fn positive_prefix_count(&self) -> usize {
        self.verified_positive_prefixes.len()
    }
}

impl CompleteTurnPublicPrefixScorerV1 for CompleteTurnPositivePrefixEvidenceV1 {
    fn supports_root(&self, root: &CompleteTurnPublicPrefixStateV1) -> bool {
        self.root == *root
    }

    fn guide_ranks(
        &self,
        state: &CompleteTurnPublicPrefixStateV1,
    ) -> Vec<CompleteTurnPrefixGuideRankV1> {
        if self.verified_positive_prefixes.contains(state) {
            vec![CompleteTurnPrefixGuideRankV1 {
                lane: COMPLETE_TURN_POSITIVE_PREFIX_GUIDE_LANE_V1,
                components: vec![1],
            }]
        } else {
            Vec::new()
        }
    }
}

pub fn project_complete_turn_public_prefix_state_v1(
    position: &CombatPosition,
) -> Result<CompleteTurnPublicPrefixStateV1, String> {
    let terminal = combat_terminal(&position.engine, &position.combat);
    let boundary = match terminal {
        CombatTerminal::Win => CompleteTurnPublicPrefixBoundaryV1::TerminalWin,
        CombatTerminal::Loss => CompleteTurnPublicPrefixBoundaryV1::TerminalLoss,
        CombatTerminal::Unresolved => {
            let projection =
                project_public_combat_actions_v1(&position.engine, &position.combat)
                    .map_err(|error| format!("public prefix action projection failed: {error}"))?;
            CompleteTurnPublicPrefixBoundaryV1::Decision {
                action_surface: projection.public,
            }
        }
    };
    Ok(CompleteTurnPublicPrefixStateV1 {
        observation: public_combat_state_v1(&position.combat),
        boundary,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompleteTurnObservationContractV1 {
    /// The research actor may restore the exact root, execute a proposal, and
    /// observe the resulting public state before choosing which line to commit.
    ExactBranchObservedPublicSuccessor,
    /// The research actor may observe all public partial prefixes generated at
    /// one exact action depth before choosing retained ordinals.
    ExactBranchObservedPublicPartialPrefix,
    /// The research actor may observe all already generated complete-turn
    /// endpoints before reserving which ones enter the bounded executable
    /// surface.
    ExactBranchObservedPublicCompletedEndpoint,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnPrefixCandidateViewV1 {
    pub actions: Vec<CompleteTurnSelectedActionV1>,
    pub current: CompleteTurnPublicPrefixStateV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnPrefixLayerViewV1 {
    pub schema: String,
    pub observation_contract: CompleteTurnObservationContractV1,
    pub root: CompleteTurnPublicPrefixStateV1,
    pub atomic_depth: usize,
    pub max_retained_states: usize,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub candidates: Vec<CompleteTurnPrefixCandidateViewV1>,
}

pub fn project_complete_turn_prefix_layer_v1(
    root: &CombatPosition,
    layer: &ExactCompleteTurnPrefixLayerV1,
    max_engine_steps_per_action: usize,
) -> Result<CompleteTurnPrefixLayerViewV1, String> {
    let public_root = project_complete_turn_public_prefix_state_v1(root)?;
    let candidates = layer.try_project_candidates(|inputs, exact_current| {
        let (actions, replayed) = replay_and_project_actions(
            root,
            inputs.iter().map(|input| (input, None)),
            max_engine_steps_per_action,
            "complete-turn prefix",
        )?;
        if combat_exact_state_hash_v2(&replayed.engine, &replayed.combat)
            != combat_exact_state_hash_v2(&exact_current.engine, &exact_current.combat)
        {
            return Err("complete-turn prefix final exact state drifted".to_owned());
        }
        Ok(CompleteTurnPrefixCandidateViewV1 {
            actions,
            current: project_complete_turn_public_prefix_state_v1(&replayed)?,
        })
    })?;
    Ok(CompleteTurnPrefixLayerViewV1 {
        schema: COMPLETE_TURN_PREFIX_LAYER_VIEW_SCHEMA.to_owned(),
        observation_contract:
            CompleteTurnObservationContractV1::ExactBranchObservedPublicPartialPrefix,
        root: public_root,
        atomic_depth: layer.atomic_depth(),
        max_retained_states: layer.max_retained_states(),
        nodes_expanded: layer.nodes_expanded(),
        nodes_generated: layer.nodes_generated(),
        candidates,
    })
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnCompletedEndpointLayerViewV1 {
    pub schema: String,
    pub observation_contract: CompleteTurnObservationContractV1,
    pub root: PublicCombatStateV1,
    pub root_plan: Option<CombatPlanProjectionV1>,
    pub max_retained_states: usize,
    pub candidates: Vec<CompleteTurnCandidateViewV1>,
}

pub fn project_complete_turn_completed_endpoint_layer_v1(
    root: &CombatPosition,
    layer: &ExactCompleteTurnCompletedEndpointLayerV1,
    max_engine_steps_per_action: usize,
) -> Result<CompleteTurnCompletedEndpointLayerViewV1, String> {
    let candidates = layer.try_project_candidates(|candidate, guides| {
        project_complete_turn_candidate_v1(root, candidate, guides, max_engine_steps_per_action)
    })?;
    Ok(CompleteTurnCompletedEndpointLayerViewV1 {
        schema: COMPLETE_TURN_COMPLETED_ENDPOINT_LAYER_VIEW_SCHEMA.to_owned(),
        observation_contract:
            CompleteTurnObservationContractV1::ExactBranchObservedPublicCompletedEndpoint,
        root: public_combat_state_v1(&root.combat),
        root_plan: combat_plan_projection_v1(root),
        max_retained_states: layer.max_retained_states(),
        candidates,
    })
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnDecisionViewV1 {
    pub schema: String,
    pub observation_contract: CompleteTurnObservationContractV1,
    pub root: PublicCombatStateV1,
    pub root_plan: Option<CombatPlanProjectionV1>,
    pub candidates: Vec<CompleteTurnCandidateViewV1>,
}

impl CompleteTurnDecisionViewV1 {
    pub fn new(root: &CombatPosition, candidates: Vec<CompleteTurnCandidateViewV1>) -> Self {
        Self {
            schema: COMPLETE_TURN_DECISION_VIEW_SCHEMA.to_owned(),
            observation_contract:
                CompleteTurnObservationContractV1::ExactBranchObservedPublicSuccessor,
            root: public_combat_state_v1(&root.combat),
            root_plan: combat_plan_projection_v1(root),
            candidates,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompleteTurnCandidateEvidenceV1 {
    VerifiedPositiveDemonstration,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnDemonstrationDecisionV1 {
    pub decision: CompleteTurnDecisionViewV1,
    pub candidate_evidence: Vec<CompleteTurnCandidateEvidenceV1>,
    pub chosen_candidate_ordinal: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnWinningDemonstrationV1 {
    pub schema: String,
    pub source_case: PathBuf,
    /// Provenance only. Numeric encoders must not consume this identity.
    pub source_root_exact_state_hash: String,
    pub terminal_evidence: String,
    pub decisions: Vec<CompleteTurnDemonstrationDecisionV1>,
}

impl CompleteTurnWinningDemonstrationV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != COMPLETE_TURN_WINNING_DEMONSTRATION_SCHEMA {
            return Err(format!(
                "expected {COMPLETE_TURN_WINNING_DEMONSTRATION_SCHEMA}, got {}",
                self.schema
            ));
        }
        if self.terminal_evidence != "exact_replay_win" {
            return Err("complete-turn demonstration is not an exact replay win".to_owned());
        }
        if self.decisions.is_empty() {
            return Err("complete-turn demonstration has no decisions".to_owned());
        }
        for (row, decision) in self.decisions.iter().enumerate() {
            if decision.decision.schema != COMPLETE_TURN_DECISION_VIEW_SCHEMA {
                return Err(format!(
                    "complete-turn demonstration row {row} has wrong view schema"
                ));
            }
            let candidate_count = decision.decision.candidates.len();
            if candidate_count == 0 {
                return Err(format!(
                    "complete-turn demonstration row {row} has no candidates"
                ));
            }
            if decision.candidate_evidence.len() != candidate_count {
                return Err(format!(
                    "complete-turn demonstration row {row} evidence count does not match candidates"
                ));
            }
            if decision.chosen_candidate_ordinal >= candidate_count {
                return Err(format!(
                    "complete-turn demonstration row {row} chosen ordinal is out of range"
                ));
            }
            let positive_count = decision
                .candidate_evidence
                .iter()
                .filter(|evidence| {
                    **evidence == CompleteTurnCandidateEvidenceV1::VerifiedPositiveDemonstration
                })
                .count();
            if positive_count != 1
                || decision.candidate_evidence[decision.chosen_candidate_ordinal]
                    != CompleteTurnCandidateEvidenceV1::VerifiedPositiveDemonstration
            {
                return Err(format!(
                    "complete-turn demonstration row {row} must bind exactly one positive to its chosen ordinal"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnCandidateViewV1 {
    pub actions: Vec<CompleteTurnSelectedActionV1>,
    pub boundary: ExactCompleteTurnProposalBoundaryV1,
    pub successor: PublicCombatStateV1,
    pub successor_plan: Option<CombatPlanProjectionV1>,
    pub plan_transition: Option<CombatPlanTransitionAnnotationV1>,
    pub potion_slot_mask: u64,
    pub potion_expenditures: u32,
    pub support_claims: Vec<ExactCompleteTurnProposalSupportClaimV1>,
    pub guide_ranks: Vec<CompleteTurnGuideRankV1>,
}

/// Assigns one content identity to the public complete-turn view shared by
/// proposal display, exact replay, and continuation diagnostics.
pub fn complete_turn_candidate_id_v1(view: &CompleteTurnCandidateViewV1) -> Result<String, String> {
    stable_planner_id(COMPLETE_TURN_CANDIDATE_ID_NAMESPACE_V1, view)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTurnGuideRankV1 {
    pub lane: u32,
    pub components: Vec<i32>,
}

impl From<&ExactCompleteTurnProposalGuideRankV1> for CompleteTurnGuideRankV1 {
    fn from(rank: &ExactCompleteTurnProposalGuideRankV1) -> Self {
        Self {
            lane: rank.lane,
            components: rank.components.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CompleteTurnSelectedActionV1 {
    Atomic {
        action: PublicCombatAtomicActionV1,
        card: Option<CombatLearningCardV1>,
        potion: Option<CombatLearningPotionV1>,
        target: Option<CombatLearningMonsterStateV1>,
    },
    Selection {
        family: PublicCombatSelectionFamilyV1,
        selected_domain_ordinals: Vec<usize>,
    },
}

pub fn project_complete_turn_candidate_v1(
    root: &CombatPosition,
    candidate: &ExactCompleteTurnProposalV1,
    guide_ranks: &[ExactCompleteTurnProposalGuideRankV1],
    max_engine_steps_per_action: usize,
) -> Result<CompleteTurnCandidateViewV1, String> {
    let (actions, position) = replay_and_project_actions(
        root,
        candidate.actions.iter().map(|recorded| {
            (
                &recorded.input,
                Some(recorded.expected_successor_hash.as_str()),
            )
        }),
        max_engine_steps_per_action,
        "complete-turn action",
    )?;

    if combat_exact_state_hash_v2(&position.engine, &position.combat)
        != combat_exact_state_hash_v2(&candidate.successor.engine, &candidate.successor.combat)
    {
        return Err("complete-turn candidate final exact successor drifted".to_owned());
    }

    Ok(CompleteTurnCandidateViewV1 {
        actions,
        boundary: candidate.boundary,
        successor: public_combat_state_v1(&position.combat),
        successor_plan: combat_plan_projection_v1(&position),
        plan_transition: combat_plan_transition_annotation_v1(root, &position),
        potion_slot_mask: candidate.potion_slot_mask,
        potion_expenditures: candidate.potion_expenditures,
        support_claims: candidate.support_claims.clone(),
        guide_ranks: guide_ranks
            .iter()
            .map(CompleteTurnGuideRankV1::from)
            .collect(),
    })
}

fn replay_and_project_actions<'a>(
    root: &CombatPosition,
    inputs: impl IntoIterator<Item = (&'a ClientInput, Option<&'a str>)>,
    max_engine_steps_per_action: usize,
    context: &str,
) -> Result<(Vec<CompleteTurnSelectedActionV1>, CombatPosition), String> {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut actions = Vec::new();
    for (action_index, (input, expected_successor_hash)) in inputs.into_iter().enumerate() {
        let observation = public_combat_state_v1(&position.combat);
        let projection = project_public_combat_actions_v1(&position.engine, &position.combat)
            .map_err(|error| {
                format!("{context} {action_index} public projection failed: {error}")
            })?;
        let choice = public_choice_for_exact_combat_input_v1(&projection, input)
            .map_err(|error| format!("{context} {action_index} has no public choice: {error}"))?;
        actions.push(selected_action(&observation, &projection.public, &choice)?);

        let result = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_action,
                deadline: None,
            },
        );
        if result.truncated || result.timed_out {
            return Err(format!(
                "{context} {action_index} exceeded its finite replay limit"
            ));
        }
        if let Some(expected) = expected_successor_hash {
            let actual =
                combat_exact_state_hash_v2(&result.position.engine, &result.position.combat);
            if actual != expected {
                return Err(format!("{context} {action_index} exact successor drifted"));
            }
        }
        position = result.position;
    }
    Ok((actions, position))
}

fn selected_action(
    observation: &PublicCombatStateV1,
    surface: &sts_agent::agent::information::action::PublicCombatActionSurfaceV1,
    choice: &PublicCombatActionChoiceV1,
) -> Result<CompleteTurnSelectedActionV1, String> {
    match choice {
        PublicCombatActionChoiceV1::Atomic { action_ordinal } => {
            let action = surface
                .atomic_actions
                .get(*action_ordinal)
                .cloned()
                .ok_or_else(|| "public atomic action ordinal is out of range".to_owned())?;
            let (card, potion, target) = match &action {
                PublicCombatAtomicActionV1::PlayCard {
                    hand_index,
                    target_monster_index,
                } => (
                    observation.cards.hand.cards.get(*hand_index).cloned(),
                    None,
                    target_monster_index.and_then(|index| observation.monsters.get(index).cloned()),
                ),
                PublicCombatAtomicActionV1::UsePotion {
                    potion_index,
                    target_monster_index,
                } => (
                    None,
                    observation
                        .potions
                        .get(*potion_index)
                        .and_then(Clone::clone),
                    target_monster_index.and_then(|index| observation.monsters.get(index).cloned()),
                ),
                PublicCombatAtomicActionV1::DiscardPotion { potion_index } => (
                    None,
                    observation
                        .potions
                        .get(*potion_index)
                        .and_then(Clone::clone),
                    None,
                ),
                _ => (None, None, None),
            };
            Ok(CompleteTurnSelectedActionV1::Atomic {
                action,
                card,
                potion,
                target,
            })
        }
        PublicCombatActionChoiceV1::Selection {
            family_ordinal,
            selected_domain_ordinals,
        } => {
            let family = surface
                .selection_families
                .get(*family_ordinal)
                .cloned()
                .ok_or_else(|| "public selection family ordinal is out of range".to_owned())?;
            Ok(CompleteTurnSelectedActionV1::Selection {
                family,
                selected_domain_ordinals: selected_domain_ordinals.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::{cards::CardId, monsters::EnemyId};
    use sts_core::runtime::combat::CombatCard;
    use sts_core::state::core::EngineState;
    use sts_core::test_support::{blank_test_combat, planned_monster};

    #[test]
    fn positive_prefix_evidence_guides_only_replayed_public_states() {
        let mut combat = blank_test_combat();
        let monster = planned_monster(EnemyId::JawWorm, 1);
        let target = monster.id;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let inputs = vec![
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
            ClientInput::EndTurn,
        ];
        let evidence =
            CompleteTurnPositivePrefixEvidenceV1::from_replayed_complete_turn(&root, &inputs, 250)
                .unwrap();

        assert!(evidence.supports_root(&evidence.root));
        assert_eq!(evidence.positive_prefix_count(), 2);
        assert!(evidence.guide_ranks(&evidence.root).is_empty());
        assert_eq!(
            evidence.guide_ranks(&evidence.verified_positive_prefixes[0]),
            vec![CompleteTurnPrefixGuideRankV1 {
                lane: COMPLETE_TURN_POSITIVE_PREFIX_GUIDE_LANE_V1,
                components: vec![1],
            }]
        );

        let mut unknown = evidence.verified_positive_prefixes[0].clone();
        unknown.observation.player.hp = unknown.observation.player.hp.saturating_sub(1);
        assert!(evidence.guide_ranks(&unknown).is_empty());
        assert!(!evidence.supports_root(&unknown));
    }
}
