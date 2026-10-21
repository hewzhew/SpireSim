//! The single typed owner for bounded exact complete-turn proposals.
//!
//! A proposal batch is a sampled action subset for one exact player-turn
//! boundary. It owns same-turn candidate recall only. Cross-turn search,
//! controller memory, terminal proof, and execution selection belong to its
//! callers. The current implementation delegates enumeration to the proven
//! AtomicExactV2 turn-plan kernel while keeping that engine's configuration,
//! reports, and value types behind this neutral boundary.

use serde::{Deserialize, Serialize};
use sts_card_semantics::ai::analysis::card_semantics::{
    card_definition_with_upgrades, Mechanic, PlayEffect,
};
use sts_card_semantics::content::cards::CardType;
use sts_combat_atomic::complete_turn_kernel::{
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices_guided_with_deadline,
    CombatSearchV2ActionFacts, CombatSearchV2Config, CombatSearchV2PotionPolicy,
    CombatSearchV2TurnPlanPartialRetentionAuditV1, CombatSearchV2TurnPlanPartialRetentionLaneV1,
    CombatSearchV2TurnPlanPrefixSessionV1, CombatSearchV2TurnPlanProbeEnumeration,
    CombatSearchV2TurnPlanProbeGuideRankV1,
};
use sts_core::content::powers::PowerId;
use sts_core::runtime::combat::{CardLineageId, CombatState};
use sts_core::sim::combat::CombatPosition;
use sts_core::state::core::ClientInput;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactCompleteTurnProposalConfigV1 {
    pub max_inner_nodes: usize,
    pub max_partial_frontier_states: usize,
    pub max_end_states: usize,
    pub per_bucket_limit: usize,
    pub max_engine_steps_per_action: usize,
    pub allow_potion_expenditure: bool,
    pub max_potion_expenditures: Option<u32>,
    pub allowed_potion_slots: Option<u64>,
    pub allow_potion_discard: bool,
}

impl Default for ExactCompleteTurnProposalConfigV1 {
    fn default() -> Self {
        Self {
            max_inner_nodes: 256,
            max_partial_frontier_states: 32,
            max_end_states: 24,
            per_bucket_limit: 24,
            max_engine_steps_per_action: 250,
            allow_potion_expenditure: true,
            max_potion_expenditures: None,
            allowed_potion_slots: None,
            allow_potion_discard: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactCompleteTurnProposalBoundaryV1 {
    NextPlayerTurn,
    TerminalWin,
    TerminalLoss,
    Other,
}

#[derive(Clone, Debug)]
pub struct ExactCompleteTurnProposalActionV1 {
    pub input: ClientInput,
    pub expected_successor_hash: String,
    pub engine_steps: usize,
}

#[derive(Clone)]
pub struct ExactCompleteTurnProposalV1 {
    pub actions: Vec<ExactCompleteTurnProposalActionV1>,
    pub boundary: ExactCompleteTurnProposalBoundaryV1,
    pub successor: CombatPosition,
    /// Exact potion slots used or discarded by this complete-turn proposal.
    /// Zero is a real resource profile, not an absence of evidence.
    pub potion_slot_mask: u64,
    /// Exact potion expenditures in this complete turn. This is deliberately
    /// separate from the slot mask: one slot can be spent more than once, and
    /// a passive potion can disappear without a potion input.
    pub potion_expenditures: u32,
    pub support_claims: Vec<ExactCompleteTurnProposalSupportClaimV1>,
}

/// A small, public-mechanics support domain from which the bounded proposal
/// set should retain a representative. Claims describe recall, not value or
/// execution preference.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExactCompleteTurnProposalSupportClaimV1 {
    VisibleThreatMitigation {
        target_slot: usize,
        potion_slot_mask: u64,
    },
    DebuffProtectionRemoval {
        target_slot: usize,
        remaining_stacks: i32,
        potion_slot_mask: u64,
    },
    DurablePlayerStrengthGain {
        /// Strength realized by player actions in this proposal and still
        /// present at the successor boundary. Reactive EndTurn gains are not
        /// attributed to the proposal merely because they share a successor.
        strength_gain: i32,
        potion_slot_mask: u64,
    },
    /// A Power card paid its one-time play cost and left its combat-long
    /// effect installed. The stable card identity keeps distinct setup
    /// families visible without assigning any of them a context-free value.
    DurablePlayerPowerInstalled {
        source_card_id: String,
        potion_slot_mask: u64,
    },
    PreparedDrawAccess {
        /// Root-local exact identity of a card explicitly moved to draw-pile
        /// top and still available in the next hand or at the next draw.
        card_lineage: CardLineageId,
        potion_slot_mask: u64,
    },
    WithheldSurvivalResourcePreserved {
        holder_slot: usize,
        potion_slot_mask: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ExactCompleteTurnProposalSupportFamilyV1 {
    VisibleThreatMitigation,
    DebuffProtectionRemoval,
    DurablePlayerStrengthGain,
    DurablePlayerPowerInstalled,
    PreparedDrawAccess,
    WithheldSurvivalResourcePreserved,
}

impl ExactCompleteTurnProposalSupportClaimV1 {
    fn family(&self) -> ExactCompleteTurnProposalSupportFamilyV1 {
        match self {
            Self::VisibleThreatMitigation { .. } => {
                ExactCompleteTurnProposalSupportFamilyV1::VisibleThreatMitigation
            }
            Self::DebuffProtectionRemoval { .. } => {
                ExactCompleteTurnProposalSupportFamilyV1::DebuffProtectionRemoval
            }
            Self::DurablePlayerStrengthGain { .. } => {
                ExactCompleteTurnProposalSupportFamilyV1::DurablePlayerStrengthGain
            }
            Self::DurablePlayerPowerInstalled { .. } => {
                ExactCompleteTurnProposalSupportFamilyV1::DurablePlayerPowerInstalled
            }
            Self::PreparedDrawAccess { .. } => {
                ExactCompleteTurnProposalSupportFamilyV1::PreparedDrawAccess
            }
            Self::WithheldSurvivalResourcePreserved { .. } => {
                ExactCompleteTurnProposalSupportFamilyV1::WithheldSurvivalResourcePreserved
            }
        }
    }
}

/// One opaque, lexicographically ordered view over an already materialized
/// exact turn successor.  The proposal owner never interprets components or
/// compares ranks from different lanes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactCompleteTurnProposalGuideRankV1 {
    pub lane: u32,
    pub components: Vec<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactCompleteTurnPartialRetentionLaneV1 {
    Untruncated,
    FirstActionCoverage,
    Survival,
    Progress,
    Setup,
    PowerInstallation,
    ResourceTransformation,
    PendingChoiceContinuation,
    PendingChoiceSelection,
    Balanced,
    Guide(u32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactCompleteTurnPartialCandidateAuditV1 {
    pub selected_by: Option<ExactCompleteTurnPartialRetentionLaneV1>,
    pub survival_rank: usize,
    pub progress_rank: usize,
    pub setup_rank: usize,
    pub power_installation_rank: Option<usize>,
    pub resource_transformation_rank: Option<usize>,
    pub pending_choice_continuation_rank: Option<usize>,
    pub pending_choice_selection_rank: Option<usize>,
    pub balanced_rank: usize,
    pub first_action_balanced_rank: usize,
    pub guide_conditioned_ranks: Vec<ExactCompleteTurnPartialGuideConditionedAuditV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactCompleteTurnPartialGuideConditionedAuditV1 {
    pub lane: u32,
    pub peer_count: usize,
    pub survival_rank: usize,
    pub progress_rank: usize,
    pub setup_rank: usize,
    pub balanced_rank: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactCompleteTurnPartialRetentionAuditV1 {
    pub max_retained_states: usize,
    pub candidates: Vec<ExactCompleteTurnPartialCandidateAuditV1>,
}

impl From<CombatSearchV2TurnPlanPartialRetentionAuditV1>
    for ExactCompleteTurnPartialRetentionAuditV1
{
    fn from(audit: CombatSearchV2TurnPlanPartialRetentionAuditV1) -> Self {
        Self {
            max_retained_states: audit.max_retained_states,
            candidates: audit
                .candidates
                .into_iter()
                .map(|candidate| ExactCompleteTurnPartialCandidateAuditV1 {
                    selected_by: candidate.selected_by.map(|lane| match lane {
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::Untruncated => {
                            ExactCompleteTurnPartialRetentionLaneV1::Untruncated
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::FirstActionCoverage => {
                            ExactCompleteTurnPartialRetentionLaneV1::FirstActionCoverage
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::Survival => {
                            ExactCompleteTurnPartialRetentionLaneV1::Survival
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::Progress => {
                            ExactCompleteTurnPartialRetentionLaneV1::Progress
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::Setup => {
                            ExactCompleteTurnPartialRetentionLaneV1::Setup
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::PowerInstallation => {
                            ExactCompleteTurnPartialRetentionLaneV1::PowerInstallation
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::ResourceTransformation => {
                            ExactCompleteTurnPartialRetentionLaneV1::ResourceTransformation
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::PendingChoiceContinuation => {
                            ExactCompleteTurnPartialRetentionLaneV1::PendingChoiceContinuation
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::PendingChoiceSelection => {
                            ExactCompleteTurnPartialRetentionLaneV1::PendingChoiceSelection
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::Balanced => {
                            ExactCompleteTurnPartialRetentionLaneV1::Balanced
                        }
                        CombatSearchV2TurnPlanPartialRetentionLaneV1::Guide(lane) => {
                            ExactCompleteTurnPartialRetentionLaneV1::Guide(lane)
                        }
                    }),
                    survival_rank: candidate.survival_rank,
                    progress_rank: candidate.progress_rank,
                    setup_rank: candidate.setup_rank,
                    power_installation_rank: candidate.power_installation_rank,
                    resource_transformation_rank: candidate.resource_transformation_rank,
                    pending_choice_continuation_rank: candidate.pending_choice_continuation_rank,
                    pending_choice_selection_rank: candidate.pending_choice_selection_rank,
                    balanced_rank: candidate.balanced_rank,
                    first_action_balanced_rank: candidate.first_action_balanced_rank,
                    guide_conditioned_ranks: candidate
                        .guide_conditioned_ranks
                        .into_iter()
                        .map(
                            |conditioned| ExactCompleteTurnPartialGuideConditionedAuditV1 {
                                lane: conditioned.lane,
                                peer_count: conditioned.peer_count,
                                survival_rank: conditioned.survival_rank,
                                progress_rank: conditioned.progress_rank,
                                setup_rank: conditioned.setup_rank,
                                balanced_rank: conditioned.balanced_rank,
                            },
                        )
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Clone)]
pub struct ExactCompleteTurnProposalBatchV1 {
    pub candidates: Vec<ExactCompleteTurnProposalV1>,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub engine_steps: usize,
    pub exact_state_skips: usize,
    pub truncated_children: usize,
    pub preselection_candidate_count: usize,
    pub completed_endpoint_reserved_candidate_count: usize,
    pub resource_profile_retained_candidate_count: usize,
    pub support_retained_candidate_count: usize,
}

/// Every already materialized complete-turn endpoint before the maintained
/// public-width selector runs. A caller may project and reserve generated
/// endpoints, but cannot create successors or execute a choice through this
/// layer.
#[derive(Clone)]
pub struct ExactCompleteTurnCompletedEndpointLayerV1 {
    config: ExactCompleteTurnProposalConfigV1,
    candidates: Vec<RankedProposalV1>,
    nodes_expanded: usize,
    nodes_generated: usize,
    engine_steps: usize,
    exact_state_skips: usize,
    truncated_children: usize,
    preselection_candidate_count: usize,
}

impl ExactCompleteTurnCompletedEndpointLayerV1 {
    pub fn max_retained_states(&self) -> usize {
        self.config.max_end_states
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Projects observation data without transferring ownership of exact
    /// candidates to the caller. Returned values must not preserve private
    /// identity.
    pub fn try_project_candidates<T>(
        &self,
        mut project: impl FnMut(
            &ExactCompleteTurnProposalV1,
            &[ExactCompleteTurnProposalGuideRankV1],
        ) -> Result<T, String>,
    ) -> Result<Vec<T>, String> {
        self.candidates
            .iter()
            .map(|candidate| project(&candidate.proposal, &candidate.guides))
            .collect()
    }

    /// Reserves observation-local endpoint ordinals first, then fills only
    /// unused capacity with the maintained completed-state selector.
    pub fn retain_with_default_fill(
        &self,
        reserved_ordinals: &[usize],
    ) -> Result<ExactCompleteTurnProposalBatchV1, String> {
        let (
            selected_indexes,
            resource_profile_retained_candidate_count,
            support_retained_candidate_count,
        ) = select_candidate_indexes_with_reservations(
            &self.candidates,
            self.config.max_end_states,
            self.config.per_bucket_limit,
            reserved_ordinals,
        )?;
        let completed_endpoint_reserved_candidate_count = reserved_ordinals.len();
        let mut slots = self
            .candidates
            .iter()
            .map(|candidate| Some(candidate.proposal.clone()))
            .collect::<Vec<_>>();
        let candidates = selected_indexes
            .into_iter()
            .filter_map(|index| slots[index].take())
            .collect();
        Ok(ExactCompleteTurnProposalBatchV1 {
            candidates,
            nodes_expanded: self.nodes_expanded,
            nodes_generated: self.nodes_generated,
            engine_steps: self.engine_steps,
            exact_state_skips: self.exact_state_skips,
            truncated_children: self.truncated_children,
            preselection_candidate_count: self.preselection_candidate_count,
            completed_endpoint_reserved_candidate_count,
            resource_profile_retained_candidate_count,
            support_retained_candidate_count,
        })
    }
}

/// One exact, already generated same-turn layer. Exact positions remain
/// encapsulated here; public consumers can project them through
/// `try_project_candidates` but cannot mutate the search frontier.
#[derive(Clone)]
pub struct ExactCompleteTurnPrefixLayerV1 {
    atomic_depth: usize,
    max_retained_states: usize,
    nodes_expanded: usize,
    nodes_generated: usize,
    candidates: Vec<ExactCompleteTurnPrefixCandidateV1>,
}

#[derive(Clone)]
struct ExactCompleteTurnPrefixCandidateV1 {
    inputs: Vec<ClientInput>,
    position: CombatPosition,
}

impl ExactCompleteTurnPrefixLayerV1 {
    pub fn atomic_depth(&self) -> usize {
        self.atomic_depth
    }

    pub fn max_retained_states(&self) -> usize {
        self.max_retained_states
    }

    pub fn nodes_expanded(&self) -> usize {
        self.nodes_expanded
    }

    pub fn nodes_generated(&self) -> usize {
        self.nodes_generated
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Projects observation data without transferring exact candidate handles
    /// to the caller. Returned values must not preserve private identity.
    pub fn try_project_candidates<T>(
        &self,
        mut project: impl FnMut(&[ClientInput], &CombatPosition) -> Result<T, String>,
    ) -> Result<Vec<T>, String> {
        self.candidates
            .iter()
            .map(|candidate| project(&candidate.inputs, &candidate.position))
            .collect()
    }
}

/// Resumable proposal generation. Rust continues to own exact transitions,
/// equivalence, deduplication and final endpoint selection; the caller chooses
/// only observation-local ordinals retained between action depths.
#[derive(Clone)]
pub struct ExactCompleteTurnPrefixSessionV1 {
    root: CombatPosition,
    config: ExactCompleteTurnProposalConfigV1,
    inner: CombatSearchV2TurnPlanPrefixSessionV1,
}

impl ExactCompleteTurnPrefixSessionV1 {
    pub fn new(root: &CombatPosition, config: ExactCompleteTurnProposalConfigV1) -> Self {
        let internal = internal_turn_plan_config(config);
        Self {
            root: root.clone(),
            config,
            inner: CombatSearchV2TurnPlanPrefixSessionV1::new(
                &root.engine,
                &root.combat,
                &internal,
            ),
        }
    }

    pub fn observe_layer(&mut self) -> Option<ExactCompleteTurnPrefixLayerV1> {
        self.inner
            .observe_layer()
            .map(|layer| ExactCompleteTurnPrefixLayerV1 {
                atomic_depth: layer.atomic_depth,
                max_retained_states: layer.max_retained_states,
                nodes_expanded: layer.nodes_expanded,
                nodes_generated: layer.nodes_generated,
                candidates: layer
                    .candidates
                    .into_iter()
                    .map(|candidate| ExactCompleteTurnPrefixCandidateV1 {
                        inputs: candidate.inputs,
                        position: candidate.position,
                    })
                    .collect(),
            })
    }

    pub fn retain_layer(&mut self, ordinals: &[usize]) -> Result<(), String> {
        self.inner.retain_layer(ordinals)
    }

    /// Reserves externally selected prefixes, then fills only the remaining
    /// capacity with the maintained mechanical-diversity selector.
    pub fn retain_layer_with_default_fill(&mut self, ordinals: &[usize]) -> Result<(), String> {
        self.inner.retain_layer_with_heuristic_fill(ordinals)
    }

    /// Retains the current layer with the same guided selector used by
    /// one-shot production enumeration.
    pub fn retain_layer_with_guides(
        &mut self,
        partial_guide_ranks: &dyn Fn(&CombatPosition) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
    ) -> ExactCompleteTurnPartialRetentionAuditV1 {
        self.inner
            .retain_layer_with_guides(&|partial| {
                partial_guide_ranks(partial)
                    .into_iter()
                    .map(|rank| CombatSearchV2TurnPlanProbeGuideRankV1 {
                        lane: rank.lane,
                        components: rank.components,
                    })
                    .collect()
            })
            .into()
    }

    pub fn finish(
        self,
        completed_guide_ranks: &dyn Fn(
            &CombatPosition,
        ) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
    ) -> Result<ExactCompleteTurnProposalBatchV1, String> {
        self.finish_completed_endpoints(completed_guide_ranks)?
            .retain_with_default_fill(&[])
    }

    /// Finishes exact generation without yet applying the public endpoint
    /// width selector. This is the typed reservation boundary between partial
    /// prefix retention and the later executable surface.
    pub fn finish_completed_endpoints(
        self,
        completed_guide_ranks: &dyn Fn(
            &CombatPosition,
        ) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
    ) -> Result<ExactCompleteTurnCompletedEndpointLayerV1, String> {
        let enumeration = self.inner.finish()?;
        Ok(completed_endpoint_layer_from_enumeration(
            &self.root,
            self.config,
            enumeration,
            completed_guide_ranks,
        ))
    }
}

pub fn exact_complete_turn_proposals_v1(
    root: &CombatPosition,
    config: ExactCompleteTurnProposalConfigV1,
) -> ExactCompleteTurnProposalBatchV1 {
    exact_complete_turn_proposals_guided_v1(root, config, &|_| Vec::new())
}

/// Enumerates a bounded exact same-turn surface, then reserves at most one
/// best representative per opaque guide lane before filling the remaining
/// slots in the legacy mechanical-diversity order.
///
/// Guides cannot create, delete, or modify an exact successor.  They only see
/// successors after complete-turn materialization and therefore cannot steer
/// partial action enumeration.
pub fn exact_complete_turn_proposals_guided_v1(
    root: &CombatPosition,
    config: ExactCompleteTurnProposalConfigV1,
    guide_ranks: &dyn Fn(&CombatPosition) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
) -> ExactCompleteTurnProposalBatchV1 {
    exact_complete_turn_proposals_guided_with_deadline_v1(root, config, None, guide_ranks)
}

pub fn exact_complete_turn_proposals_guided_with_deadline_v1(
    root: &CombatPosition,
    config: ExactCompleteTurnProposalConfigV1,
    deadline: Option<Instant>,
    guide_ranks: &dyn Fn(&CombatPosition) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
) -> ExactCompleteTurnProposalBatchV1 {
    exact_complete_turn_proposals_with_partial_guides_and_deadline_v1(
        root,
        config,
        deadline,
        &|_| Vec::new(),
        guide_ranks,
    )
}

/// Enumerates one bounded same-turn surface while allowing a caller-owned
/// guide to reserve representatives during partial-prefix pruning.
///
/// Partial and completed-state guides remain separate. Both receive exact
/// positions because this crate owns exact enumeration, but their ranks are
/// opaque here: callers must project away private identity and RNG before
/// assigning meaning to a lane. A guide can retain an already generated
/// prefix; it cannot create an action, change legality, or make a terminal
/// claim.
pub fn exact_complete_turn_proposals_with_partial_guides_v1(
    root: &CombatPosition,
    config: ExactCompleteTurnProposalConfigV1,
    partial_guide_ranks: &dyn Fn(&CombatPosition) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
    completed_guide_ranks: &dyn Fn(&CombatPosition) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
) -> ExactCompleteTurnProposalBatchV1 {
    exact_complete_turn_proposals_with_partial_guides_and_deadline_v1(
        root,
        config,
        None,
        partial_guide_ranks,
        completed_guide_ranks,
    )
}

/// Deadline-aware exact proposal enumeration for bounded cross-turn callers.
/// The structural config remains reusable; the absolute deadline belongs to
/// the caller's current work quantum and is never persisted as proposal state.
pub fn exact_complete_turn_proposals_with_partial_guides_and_deadline_v1(
    root: &CombatPosition,
    config: ExactCompleteTurnProposalConfigV1,
    deadline: Option<Instant>,
    partial_guide_ranks: &dyn Fn(&CombatPosition) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
    completed_guide_ranks: &dyn Fn(&CombatPosition) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
) -> ExactCompleteTurnProposalBatchV1 {
    let internal = internal_turn_plan_config(config);
    let enumeration = enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices_guided_with_deadline(
        &root.engine,
        &root.combat,
        &internal,
        deadline,
        &|partial| {
            partial_guide_ranks(partial)
                .into_iter()
                .map(|rank| CombatSearchV2TurnPlanProbeGuideRankV1 {
                    lane: rank.lane,
                    components: rank.components,
                })
                .collect()
        },
    );
    completed_endpoint_layer_from_enumeration(root, config, enumeration, completed_guide_ranks)
        .retain_with_default_fill(&[])
        .expect("empty endpoint reservation is valid")
}

fn internal_turn_plan_config(config: ExactCompleteTurnProposalConfigV1) -> CombatSearchV2Config {
    // The legacy kernel still owns exact bounded enumeration and its stable
    // mechanical-diversity ordering. Selection to the public proposal width
    // belongs here, after callers can inspect complete successor states.
    CombatSearchV2Config {
        max_engine_steps_per_action: config.max_engine_steps_per_action,
        turn_plan_probe_max_inner_nodes: Some(config.max_inner_nodes),
        turn_plan_probe_max_partial_frontier_states: Some(config.max_partial_frontier_states),
        turn_plan_probe_max_end_states: Some(usize::MAX),
        turn_plan_probe_per_bucket_limit: Some(usize::MAX),
        potion_policy: if config.allow_potion_expenditure {
            CombatSearchV2PotionPolicy::All
        } else {
            CombatSearchV2PotionPolicy::Never
        },
        max_potions_used: config.max_potion_expenditures,
        allowed_potion_slots: config.allowed_potion_slots,
        allow_potion_discard: Some(config.allow_potion_discard),
        input_label: Some("exact_complete_turn_proposal_v1".to_owned()),
        ..Default::default()
    }
}

fn completed_endpoint_layer_from_enumeration(
    root: &CombatPosition,
    config: ExactCompleteTurnProposalConfigV1,
    enumeration: CombatSearchV2TurnPlanProbeEnumeration,
    completed_guide_ranks: &dyn Fn(&CombatPosition) -> Vec<ExactCompleteTurnProposalGuideRankV1>,
) -> ExactCompleteTurnCompletedEndpointLayerV1 {
    let report = &enumeration.report.enumeration;
    let mut candidates = enumeration
        .candidates
        .into_iter()
        .map(|candidate| {
            let bucket = candidate.report.bucket;
            let boundary = match candidate.report.stop_reason {
                "next_turn" => ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn,
                "terminal" if candidate.report.outcome_class == "win" => {
                    ExactCompleteTurnProposalBoundaryV1::TerminalWin
                }
                "terminal" if candidate.report.outcome_class == "loss" => {
                    ExactCompleteTurnProposalBoundaryV1::TerminalLoss
                }
                _ => ExactCompleteTurnProposalBoundaryV1::Other,
            };
            let (potion_slot_mask, potion_expenditures) = complete_turn_potion_usage_v1(
                root,
                &candidate.position,
                candidate.report.actions.iter().map(|action| &action.input),
            );
            let support_claims = support_claims(
                root,
                &candidate.position,
                &candidate.report.action_facts,
                potion_slot_mask,
            );
            let proposal = ExactCompleteTurnProposalV1 {
                actions: candidate
                    .report
                    .steps
                    .into_iter()
                    .map(|step| ExactCompleteTurnProposalActionV1 {
                        input: step.action.input,
                        expected_successor_hash: step.state_after_exact_state_hash,
                        engine_steps: step.engine_steps,
                    })
                    .collect(),
                boundary,
                successor: candidate.position,
                potion_slot_mask,
                potion_expenditures,
                support_claims,
            };
            let guides = completed_guide_ranks(&proposal.successor);
            RankedProposalV1 {
                proposal,
                bucket,
                guides,
            }
        })
        .collect::<Vec<_>>();
    retain_potion_inventory_nondominated_equivalent_endpoints(&mut candidates);
    ExactCompleteTurnCompletedEndpointLayerV1 {
        config,
        candidates,
        nodes_expanded: report.nodes_expanded,
        nodes_generated: report.nodes_generated,
        engine_steps: report.engine_steps,
        exact_state_skips: report.exact_state_skips,
        truncated_children: report.truncated_children,
        preselection_candidate_count: report.preselection_plans,
    }
}

/// Removes a complete-turn endpoint only when another generated endpoint has
/// the exact same successor after potion slots are hidden, while retaining a
/// strict superset of the same potions with no more expenditures.
///
/// This is narrower than combat-value dominance. A potion which changes HP,
/// powers, relic state, RNG, card zones, enemies, or the engine boundary keeps
/// a distinct successor. The filter only rejects mechanically redundant uses
/// such as drinking Blood Potion at full HP and otherwise taking the same
/// turn. Such a use must not consume one of the bounded public proposal slots
/// merely because potion-spending profiles receive recall reservations.
fn retain_potion_inventory_nondominated_equivalent_endpoints(
    candidates: &mut Vec<RankedProposalV1>,
) {
    let keep = (0..candidates.len())
        .map(|right| {
            !(0..candidates.len()).any(|left| {
                left != right
                    && proposal_strictly_potion_dominates(
                        &candidates[left].proposal,
                        &candidates[right].proposal,
                    )
            })
        })
        .collect::<Vec<_>>();
    let mut ordinal = 0_usize;
    candidates.retain(|_| {
        let retained = keep[ordinal];
        ordinal = ordinal.saturating_add(1);
        retained
    });
}

fn proposal_strictly_potion_dominates(
    left: &ExactCompleteTurnProposalV1,
    right: &ExactCompleteTurnProposalV1,
) -> bool {
    if left.boundary != right.boundary || left.potion_expenditures > right.potion_expenditures {
        return false;
    }
    let left_potions = &left.successor.combat.entities.potions;
    let right_potions = &right.successor.combat.entities.potions;
    if left_potions.len() != right_potions.len() {
        return false;
    }

    let mut retains_strictly_more = left.potion_expenditures < right.potion_expenditures;
    for (left_slot, right_slot) in left_potions.iter().zip(right_potions) {
        match (left_slot, right_slot) {
            (Some(left_potion), Some(right_potion)) if left_potion == right_potion => {}
            (Some(_), None) => retains_strictly_more = true,
            (None, None) => {}
            _ => return false,
        }
    }
    if !retains_strictly_more {
        return false;
    }

    let mut left_without_potions = left.successor.clone();
    let mut right_without_potions = right.successor.clone();
    left_without_potions.combat.entities.potions.clear();
    right_without_potions.combat.entities.potions.clear();
    left_without_potions == right_without_potions
}

/// Resource usage for one complete player-turn proposal.
///
/// Explicit uses/discards are counted individually, even when the same slot
/// is reused. A starting potion that disappears without a matching explicit
/// slot action is a passive expenditure (for example Fairy in a Bottle). A
/// complete-turn proposal stops at the next player turn, so no later explicit
/// action can alias a passive end-turn expenditure in that same slot.
fn complete_turn_potion_usage_v1<'a>(
    root: &CombatPosition,
    successor: &CombatPosition,
    inputs: impl IntoIterator<Item = &'a ClientInput>,
) -> (u64, u32) {
    let remaining_potion_uuids = successor
        .combat
        .entities
        .potions
        .iter()
        .flatten()
        .map(|potion| potion.uuid)
        .collect::<BTreeSet<_>>();
    let missing_starting_slot_mask = root
        .combat
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            let potion = potion.as_ref()?;
            (!remaining_potion_uuids.contains(&potion.uuid))
                .then(|| u32::try_from(slot).ok())
                .flatten()
                .and_then(|slot| 1_u64.checked_shl(slot))
        })
        .fold(0_u64, |mask, slot| mask | slot);
    let mut explicit_slot_mask = 0_u64;
    let explicit_expenditures = inputs
        .into_iter()
        .filter(|input| {
            let slot = match input {
                ClientInput::UsePotion { potion_index, .. } => Some(*potion_index),
                ClientInput::DiscardPotion(slot) => Some(*slot),
                _ => None,
            };
            let Some(slot) = slot else {
                return false;
            };
            if let Some(mask) = u32::try_from(slot)
                .ok()
                .and_then(|slot| 1_u64.checked_shl(slot))
            {
                explicit_slot_mask |= mask;
            }
            true
        })
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    let passive_expenditures = (missing_starting_slot_mask & !explicit_slot_mask).count_ones();
    (
        explicit_slot_mask | missing_starting_slot_mask,
        explicit_expenditures.saturating_add(passive_expenditures),
    )
}

fn support_claims(
    root: &CombatPosition,
    successor: &CombatPosition,
    action_facts: &[CombatSearchV2ActionFacts],
    potion_slot_mask: u64,
) -> Vec<ExactCompleteTurnProposalSupportClaimV1> {
    let mut claims = BTreeSet::new();
    for facts in action_facts {
        let Some(target) = facts
            .target
            .as_ref()
            .filter(|target| target.visible_incoming_damage > 0)
        else {
            continue;
        };
        let direct = &facts.mechanics.direct;
        let derived = &facts.mechanics.derived;
        let mitigates_visible_threat = direct.persistent_enemy_strength_down > 0
            || direct.temporary_enemy_strength_down > 0
            || direct.visible_attack_mitigation_hint > 0
            || derived.enemy_weak > 0;
        if mitigates_visible_threat {
            claims.insert(
                ExactCompleteTurnProposalSupportClaimV1::VisibleThreatMitigation {
                    target_slot: target.target_slot,
                    potion_slot_mask,
                },
            );
        }
    }
    for (target_slot, root_monster) in root.combat.entities.monsters.iter().enumerate() {
        if !root_monster.is_alive_for_action() {
            continue;
        }
        let root_protection = root
            .combat
            .get_power(root_monster.id, PowerId::Artifact)
            .max(0);
        if root_protection == 0 {
            continue;
        }
        let Some(successor_monster) = successor
            .combat
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == root_monster.id)
            .filter(|monster| monster.is_alive_for_action())
        else {
            continue;
        };
        let successor_protection = successor
            .combat
            .get_power(successor_monster.id, PowerId::Artifact)
            .max(0);
        if successor_protection < root_protection {
            claims.insert(
                ExactCompleteTurnProposalSupportClaimV1::DebuffProtectionRemoval {
                    target_slot,
                    remaining_stacks: successor_protection,
                    potion_slot_mask,
                },
            );
        }
    }
    let strength_gain = retained_player_action_strength_gain(root, successor, action_facts);
    if strength_gain > 0 {
        claims.insert(
            ExactCompleteTurnProposalSupportClaimV1::DurablePlayerStrengthGain {
                strength_gain,
                potion_slot_mask,
            },
        );
    }
    for source_card_id in action_facts.iter().filter_map(|facts| {
        (facts.action_kind == "play_card")
            .then_some(())
            .and(facts.card.as_ref())
            .filter(|card| card.card_type == CardType::Power)
            .map(|card| card.card_id.clone())
    }) {
        claims.insert(
            ExactCompleteTurnProposalSupportClaimV1::DurablePlayerPowerInstalled {
                source_card_id,
                potion_slot_mask,
            },
        );
    }
    for card_lineage in action_facts.iter().filter_map(|facts| {
        facts
            .pending_choice_selection
            .as_ref()
            .and_then(|selection| selection.prepared_draw_card_lineage)
    }) {
        let remains_prepared = successor
            .combat
            .zones
            .hand
            .iter()
            .any(|card| card.lineage == card_lineage)
            || successor
                .combat
                .zones
                .draw_pile
                .first()
                .is_some_and(|card| card.lineage == card_lineage);
        if remains_prepared {
            claims.insert(
                ExactCompleteTurnProposalSupportClaimV1::PreparedDrawAccess {
                    card_lineage,
                    potion_slot_mask,
                },
            );
        }
    }
    for (holder_slot, root_monster) in root.combat.entities.monsters.iter().enumerate() {
        if !root_monster.is_alive_for_action()
            || withheld_block_sources_for(&root.combat, root_monster.id) == 0
        {
            continue;
        }
        let preserves_withheld_resource = successor
            .combat
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == root_monster.id)
            .is_some_and(|monster| {
                monster.is_alive_for_action()
                    && withheld_block_sources_for(&successor.combat, monster.id) > 0
            });
        if preserves_withheld_resource {
            claims.insert(
                ExactCompleteTurnProposalSupportClaimV1::WithheldSurvivalResourcePreserved {
                    holder_slot,
                    potion_slot_mask,
                },
            );
        }
    }
    claims.into_iter().collect()
}

fn retained_player_action_strength_gain(
    root: &CombatPosition,
    successor: &CombatPosition,
    action_facts: &[CombatSearchV2ActionFacts],
) -> i32 {
    let realized_by_player_actions = action_facts
        .iter()
        .filter(|facts| facts.action_kind != "end_turn")
        .map(|facts| {
            facts
                .exact_one_step_delta
                .player_strength_delta
                .saturating_sub(facts.mechanics.direct.player_temporary_strength_gain)
                .max(0)
        })
        .fold(0_i32, i32::saturating_add);
    let root_strength = root
        .combat
        .get_power(root.combat.entities.player.id, PowerId::Strength);
    let successor_strength = successor
        .combat
        .get_power(successor.combat.entities.player.id, PowerId::Strength);
    realized_by_player_actions.min(successor_strength.saturating_sub(root_strength).max(0))
}

fn withheld_block_sources_for(combat: &CombatState, holder_id: usize) -> usize {
    sts_core::content::powers::store::powers_for(combat, holder_id)
        .unwrap_or_default()
        .iter()
        .filter(|power| power.power_type == PowerId::Stasis)
        .filter(|power| {
            let Some(held_lineage) =
                sts_core::content::powers::core::stasis::held_card_lineage(power.extra_data)
            else {
                return false;
            };
            combat.zones.limbo.iter().any(|card| {
                card.lineage == held_lineage
                    && card_definition_with_upgrades(card.id, card.upgrades)
                        .play_effects
                        .contains(&PlayEffect::Provide(Mechanic::Block))
            })
        })
        .count()
}

#[derive(Clone)]
struct RankedProposalV1 {
    proposal: ExactCompleteTurnProposalV1,
    bucket: &'static str,
    guides: Vec<ExactCompleteTurnProposalGuideRankV1>,
}

#[cfg(test)]
fn select_candidate_indexes(
    candidates: &[RankedProposalV1],
    max_end_states: usize,
    per_bucket_limit: usize,
) -> (Vec<usize>, usize, usize) {
    select_candidate_indexes_with_reservations(candidates, max_end_states, per_bucket_limit, &[])
        .expect("empty endpoint reservation is valid")
}

fn select_candidate_indexes_with_reservations(
    candidates: &[RankedProposalV1],
    max_end_states: usize,
    per_bucket_limit: usize,
    reserved_ordinals: &[usize],
) -> Result<(Vec<usize>, usize, usize), String> {
    if reserved_ordinals.len() > max_end_states {
        return Err(format!(
            "completed endpoint reservation has {} ordinals but capacity is {max_end_states}",
            reserved_ordinals.len()
        ));
    }
    if max_end_states == 0 || per_bucket_limit == 0 || candidates.is_empty() {
        if reserved_ordinals.is_empty() {
            return Ok((Vec::new(), 0, 0));
        }
        return Err("completed endpoint reservation cannot target an empty surface".to_owned());
    }

    let mut selected = Vec::with_capacity(max_end_states.min(candidates.len()));
    let mut selected_flags = vec![false; candidates.len()];
    let mut bucket_counts = BTreeMap::<&'static str, usize>::new();
    let mut anchored_buckets = BTreeSet::new();

    for &index in reserved_ordinals {
        let candidate = candidates.get(index).ok_or_else(|| {
            format!(
                "completed endpoint reservation ordinal {index} is outside {} candidates",
                candidates.len()
            )
        })?;
        if selected_flags[index] {
            return Err(format!(
                "completed endpoint reservation repeats ordinal {index}"
            ));
        }
        selected_flags[index] = true;
        selected.push(index);
        *bucket_counts.entry(candidate.bucket).or_default() += 1;
        anchored_buckets.insert(candidate.bucket);
    }

    // Preserve one stable anchor from each existing outcome bucket before
    // support sampling. In particular, a terminal win cannot be displaced by
    // a large number of non-terminal support claims.
    for (index, candidate) in candidates.iter().enumerate() {
        if selected.len() >= max_end_states {
            break;
        }
        if !anchored_buckets.insert(candidate.bucket) {
            continue;
        }
        let count = bucket_counts.entry(candidate.bucket).or_default();
        if *count >= per_bucket_limit {
            continue;
        }
        selected_flags[index] = true;
        *count = count.saturating_add(1);
        selected.push(index);
    }

    // A no-potion complete turn and a potion-spending complete turn are
    // distinct resource profiles even when neither carries a narrow semantic
    // claim. Reserve one best opaque-guide representative for every profile
    // before semantic support sampling.
    let mut best_by_resource_profile = BTreeMap::<(u64, u32, u32), (Vec<i32>, usize)>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        for guide in &candidate.guides {
            let key = (
                candidate.proposal.potion_slot_mask,
                candidate.proposal.potion_expenditures,
                guide.lane,
            );
            let replace = best_by_resource_profile
                .get(&key)
                .is_none_or(|(rank, best_index)| {
                    guide.components > *rank || (guide.components == *rank && index < *best_index)
                });
            if replace {
                best_by_resource_profile.insert(key, (guide.components.clone(), index));
            }
        }
    }
    let selected_before_resource_profiles = selected.len();
    for (_, (_, index)) in best_by_resource_profile {
        if selected.len() >= max_end_states {
            break;
        }
        if selected_flags[index] {
            continue;
        }
        let count = bucket_counts.entry(candidates[index].bucket).or_default();
        if *count >= per_bucket_limit {
            continue;
        }
        selected_flags[index] = true;
        *count = count.saturating_add(1);
        selected.push(index);
    }
    let resource_profile_retained_candidate_count = selected
        .len()
        .saturating_sub(selected_before_resource_profiles);

    let mut best_by_support =
        BTreeMap::<(ExactCompleteTurnProposalSupportClaimV1, u32), (Vec<i32>, usize)>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        for claim in &candidate.proposal.support_claims {
            for guide in &candidate.guides {
                let key = (claim.clone(), guide.lane);
                let replace = best_by_support.get(&key).is_none_or(|(rank, best_index)| {
                    guide.components > *rank || (guide.components == *rank && index < *best_index)
                });
                if replace {
                    best_by_support.insert(key, (guide.components.clone(), index));
                }
            }
        }
    }

    // Each concrete support claim receives one recall representative here;
    // the later guide rounds remain free to retain additional endpoints. A
    // claim's representative is the exact endpoint which wins the most
    // independent opaque guide lanes, avoiding any comparison of components
    // between lanes. Interleave concrete claims by semantic family so many
    // strength amounts or target slots cannot starve every later family.
    let mut guide_wins_by_claim =
        BTreeMap::<ExactCompleteTurnProposalSupportClaimV1, BTreeMap<usize, usize>>::new();
    for ((claim, _), (_, index)) in best_by_support {
        *guide_wins_by_claim
            .entry(claim)
            .or_default()
            .entry(index)
            .or_default() += 1;
    }
    let mut support_representatives_by_family = BTreeMap::<
        ExactCompleteTurnProposalSupportFamilyV1,
        Vec<(ExactCompleteTurnProposalSupportClaimV1, usize)>,
    >::new();
    for (claim, guide_wins) in guide_wins_by_claim {
        let Some((index, _)) =
            guide_wins
                .into_iter()
                .max_by(|(left_index, left_wins), (right_index, right_wins)| {
                    left_wins
                        .cmp(right_wins)
                        .then_with(|| right_index.cmp(left_index))
                })
        else {
            continue;
        };
        support_representatives_by_family
            .entry(claim.family())
            .or_default()
            .push((claim, index));
    }
    let maximum_support_family_depth = support_representatives_by_family
        .values()
        .map(Vec::len)
        .max()
        .unwrap_or_default();
    let selected_before_support = selected.len();
    'support_rounds: for rank in 0..maximum_support_family_depth {
        for representatives in support_representatives_by_family.values() {
            let Some((_, index)) = representatives.get(rank) else {
                continue;
            };
            let index = *index;
            if selected_flags[index] {
                continue;
            }
            let count = bucket_counts.entry(candidates[index].bucket).or_default();
            if *count >= per_bucket_limit {
                continue;
            }
            selected_flags[index] = true;
            *count = count.saturating_add(1);
            selected.push(index);
            if selected.len() >= max_end_states {
                break 'support_rounds;
            }
        }
    }
    let support_retained_candidate_count = selected.len().saturating_sub(selected_before_support);

    // Continue serving every independent guide lane after its first resource
    // and support representatives. A single best endpoint per lane is not a
    // sufficient bounded surface: several strategically distinct successors
    // can tie on a coarse guide, and the later cross-turn planner needs more
    // than enumeration order to decide which of them deserves continuation.
    let mut guide_lanes = BTreeMap::<(u64, u32, u32), Vec<usize>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        for guide in &candidate.guides {
            guide_lanes
                .entry((
                    candidate.proposal.potion_slot_mask,
                    candidate.proposal.potion_expenditures,
                    guide.lane,
                ))
                .or_default()
                .push(index);
        }
    }
    for ((_, _, lane), indexes) in &mut guide_lanes {
        indexes.sort_by(|left, right| {
            candidates[*right]
                .guides
                .iter()
                .find(|guide| guide.lane == *lane)
                .map(|guide| &guide.components)
                .cmp(
                    &candidates[*left]
                        .guides
                        .iter()
                        .find(|guide| guide.lane == *lane)
                        .map(|guide| &guide.components),
                )
                .then_with(|| left.cmp(right))
        });
    }
    let maximum_guide_depth = guide_lanes.values().map(Vec::len).max().unwrap_or_default();
    'guide_rounds: for rank in 0..maximum_guide_depth {
        for indexes in guide_lanes.values() {
            let Some(index) = indexes.get(rank).copied() else {
                continue;
            };
            if selected_flags[index] {
                continue;
            }
            let count = bucket_counts.entry(candidates[index].bucket).or_default();
            if *count >= per_bucket_limit {
                continue;
            }
            selected_flags[index] = true;
            *count = count.saturating_add(1);
            selected.push(index);
            if selected.len() >= max_end_states {
                break 'guide_rounds;
            }
        }
    }

    for (index, candidate) in candidates.iter().enumerate() {
        if selected.len() >= max_end_states {
            break;
        }
        if selected_flags[index] {
            continue;
        }
        let count = bucket_counts.entry(candidate.bucket).or_default();
        if *count >= per_bucket_limit {
            continue;
        }
        selected_flags[index] = true;
        *count = count.saturating_add(1);
        selected.push(index);
    }
    Ok((
        selected,
        resource_profile_retained_candidate_count,
        support_retained_candidate_count,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_combat_atomic::complete_turn_kernel::{
        CombatSearchV2ActionCardFacts, CombatSearchV2ActionExactDeltaFacts,
        CombatSearchV2ActionImmediateFacts, CombatSearchV2ActionMechanicsFacts,
        CombatSearchV2ActionPendingChoiceSelectionFacts,
        CombatSearchV2ActionPendingChoiceSelectionRole, SearchTerminalLabel,
    };
    use sts_core::content::{
        cards::CardId,
        monsters::EnemyId,
        potions::{Potion, PotionId},
        powers::PowerId,
    };
    use sts_core::runtime::combat::{CombatCard, Power, PowerPayload};
    use sts_core::state::core::EngineState;
    use sts_core::test_support::{blank_test_combat, planned_monster};

    fn proposal(
        bucket: &'static str,
        claims: &[ExactCompleteTurnProposalSupportClaimV1],
        guides: &[(u32, &[i32])],
    ) -> RankedProposalV1 {
        RankedProposalV1 {
            proposal: ExactCompleteTurnProposalV1 {
                actions: Vec::new(),
                boundary: ExactCompleteTurnProposalBoundaryV1::NextPlayerTurn,
                successor: CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat()),
                potion_slot_mask: 0,
                potion_expenditures: 0,
                support_claims: claims.to_vec(),
            },
            bucket,
            guides: guides
                .iter()
                .map(|(lane, components)| ExactCompleteTurnProposalGuideRankV1 {
                    lane: *lane,
                    components: components.to_vec(),
                })
                .collect(),
        }
    }

    fn strength_delta_fact(
        action_kind: &'static str,
        strength_delta: i32,
    ) -> CombatSearchV2ActionFacts {
        CombatSearchV2ActionFacts {
            action_kind,
            card: None,
            target: None,
            pending_choice_selection: None,
            immediate: CombatSearchV2ActionImmediateFacts::default(),
            mechanics: CombatSearchV2ActionMechanicsFacts::default(),
            exact_one_step_delta: CombatSearchV2ActionExactDeltaFacts {
                status: "stable",
                terminal: SearchTerminalLabel::Unresolved,
                engine_steps: 1,
                player_hp_delta: 0,
                player_block_delta: 0,
                player_strength_delta: strength_delta,
                energy_delta: 0,
                hand_delta: 0,
                draw_delta: 0,
                discard_delta: 0,
                exhaust_delta: 0,
                limbo_delta: 0,
                queued_cards_delta: 0,
                total_enemy_hp_delta: 0,
                total_enemy_block_delta: 0,
                pending_choice_present: false,
                pending_choice_estimated_action_fanout: 0,
            },
        }
    }

    fn power_card_fact(action_kind: &'static str, card_id: CardId) -> CombatSearchV2ActionFacts {
        let definition = sts_core::content::cards::get_card_definition(card_id);
        let mut fact = strength_delta_fact(action_kind, 0);
        fact.card = Some(CombatSearchV2ActionCardFacts {
            hand_index: 0,
            lineage: sts_core::test_support::scenario_lineage(17),
            card_id: card_id.stable_id().to_owned(),
            name: definition.name,
            upgraded: false,
            card_type: definition.card_type,
            definition_target: definition.target,
            effective_target: definition.target,
            cost_for_turn: definition.cost.into(),
            base_cost: definition.cost,
            evaluated_damage: definition.base_damage,
            evaluated_block: definition.base_block,
            evaluated_magic: definition.base_magic,
            exhaust: definition.exhaust,
            ethereal: definition.ethereal,
            innate: definition.innate,
        });
        fact
    }

    #[test]
    fn each_support_domain_reserves_its_best_opaque_rank_before_mechanical_fill() {
        let left = ExactCompleteTurnProposalSupportClaimV1::VisibleThreatMitigation {
            target_slot: 0,
            potion_slot_mask: 0,
        };
        let right = ExactCompleteTurnProposalSupportClaimV1::VisibleThreatMitigation {
            target_slot: 2,
            potion_slot_mask: 0,
        };
        let candidates = vec![
            proposal("balanced", std::slice::from_ref(&left), &[(7, &[1])]),
            proposal("balanced", &[left], &[(7, &[9])]),
            proposal("balanced", &[right], &[(7, &[2])]),
            proposal("balanced", &[], &[]),
        ];

        let (selected, resource_count, support_count) = select_candidate_indexes(&candidates, 3, 3);

        assert_eq!(selected, vec![0, 1, 2]);
        assert_eq!(resource_count, 1);
        assert_eq!(support_count, 1);
    }

    #[test]
    fn zero_and_nonzero_potion_profiles_each_keep_a_best_guide_representative() {
        let mut potion = proposal("balanced", &[], &[(7, &[8])]);
        potion.proposal.potion_slot_mask = 0b1;
        potion.proposal.potion_expenditures = 1;
        let candidates = vec![
            proposal("balanced", &[], &[(7, &[1])]),
            proposal("balanced", &[], &[(7, &[9])]),
            potion,
        ];

        let (selected, resource_count, support_count) = select_candidate_indexes(&candidates, 3, 3);

        assert_eq!(selected, vec![0, 1, 2]);
        assert_eq!(resource_count, 2);
        assert_eq!(support_count, 0);
    }

    #[test]
    fn redundant_potion_use_cannot_claim_a_bounded_resource_profile_slot() {
        let mut preserving = proposal("balanced", &[], &[(7, &[1])]);
        preserving.proposal.successor.combat.entities.potions = vec![
            Some(Potion::new(PotionId::HeartOfIron, 17)),
            Some(Potion::new(PotionId::BloodPotion, 18)),
        ];
        let mut wasting = preserving.clone();
        wasting.proposal.successor.combat.entities.potions[1] = None;
        wasting.proposal.potion_slot_mask = 0b10;
        wasting.proposal.potion_expenditures = 1;
        wasting.guides[0].components = vec![9];

        let mut candidates = vec![wasting, preserving];
        retain_potion_inventory_nondominated_equivalent_endpoints(&mut candidates);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].proposal.potion_slot_mask, 0);
        assert_eq!(candidates[0].proposal.potion_expenditures, 0);
        assert!(candidates[0].proposal.successor.combat.entities.potions[1].is_some());
    }

    #[test]
    fn effective_potion_use_keeps_its_distinct_successor() {
        let mut preserving = proposal("balanced", &[], &[(7, &[1])]);
        preserving.proposal.successor.combat.entities.potions =
            vec![Some(Potion::new(PotionId::BloodPotion, 18))];
        preserving
            .proposal
            .successor
            .combat
            .entities
            .player
            .current_hp = 40;
        let mut healing = preserving.clone();
        healing.proposal.successor.combat.entities.potions[0] = None;
        healing.proposal.successor.combat.entities.player.current_hp = 57;
        healing.proposal.potion_slot_mask = 0b1;
        healing.proposal.potion_expenditures = 1;

        let mut candidates = vec![healing, preserving];
        retain_potion_inventory_nondominated_equivalent_endpoints(&mut candidates);

        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn same_slot_with_different_expenditure_counts_stays_distinct() {
        let mut once = proposal("balanced", &[], &[(7, &[8])]);
        once.proposal.potion_slot_mask = 0b1;
        once.proposal.potion_expenditures = 1;
        let mut twice = proposal("balanced", &[], &[(7, &[7])]);
        twice.proposal.potion_slot_mask = 0b1;
        twice.proposal.potion_expenditures = 2;

        let (selected, resource_count, support_count) =
            select_candidate_indexes(&[once, twice], 2, 2);

        assert_eq!(selected, vec![0, 1]);
        assert_eq!(resource_count, 1);
        assert_eq!(support_count, 0);
    }

    #[test]
    fn complete_turn_potion_usage_counts_reused_slots_and_passive_loss() {
        let mut combat = blank_test_combat();
        combat.entities.potions = vec![Some(Potion::new(PotionId::Elixir, 7))];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let mut successor = root.clone();
        successor.combat.entities.potions[0] = None;

        assert_eq!(
            complete_turn_potion_usage_v1(
                &root,
                &successor,
                [
                    ClientInput::UsePotion {
                        potion_index: 0,
                        target: None,
                    },
                    ClientInput::UsePotion {
                        potion_index: 0,
                        target: None,
                    },
                ]
                .iter(),
            ),
            (0b1, 2)
        );
        assert_eq!(
            complete_turn_potion_usage_v1(&root, &successor, [].iter()),
            (0b1, 1)
        );
    }

    #[test]
    fn guides_never_bypass_public_width_or_bucket_limits() {
        let claim = ExactCompleteTurnProposalSupportClaimV1::VisibleThreatMitigation {
            target_slot: 1,
            potion_slot_mask: 0,
        };
        let candidates = vec![
            proposal("balanced", std::slice::from_ref(&claim), &[(1, &[4])]),
            proposal("balanced", std::slice::from_ref(&claim), &[(2, &[5])]),
            proposal("setup", &[claim], &[(3, &[6])]),
        ];

        let (selected, resource_count, support_count) = select_candidate_indexes(&candidates, 2, 1);

        assert_eq!(selected, vec![0, 2]);
        assert_eq!(resource_count, 0);
        assert_eq!(support_count, 0);
    }

    #[test]
    fn completed_endpoint_reservation_precedes_default_width_selection() {
        let candidates = vec![
            proposal("balanced", &[], &[]),
            proposal("setup", &[], &[]),
            proposal("balanced", &[], &[]),
        ];

        let (selected, resource_count, support_count) =
            select_candidate_indexes_with_reservations(&candidates, 2, 1, &[2]).unwrap();

        assert_eq!(selected, vec![2, 1]);
        assert_eq!(resource_count, 0);
        assert_eq!(support_count, 0);
        assert!(select_candidate_indexes_with_reservations(&candidates, 2, 1, &[2, 2]).is_err());
        assert!(select_candidate_indexes_with_reservations(&candidates, 2, 1, &[3]).is_err());
    }

    #[test]
    fn removing_public_debuff_protection_claims_separate_resource_domains() {
        let mut root = blank_test_combat();
        let mut monster = planned_monster(EnemyId::JawWorm, 1);
        monster.id = 1;
        root.entities.monsters = vec![monster];
        root.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::Artifact,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, root);
        let mut successor = root.clone();
        successor
            .combat
            .entities
            .power_db
            .get_mut(&1)
            .expect("test target has powers")[0]
            .amount = 1;

        let free = support_claims(&root, &successor, &[], 0);
        let energy_spent = support_claims(&root, &successor, &[], 0b10);
        let mut fully_exposed = successor.clone();
        fully_exposed
            .combat
            .entities
            .power_db
            .get_mut(&1)
            .expect("test target has powers")[0]
            .amount = 0;
        let completed = support_claims(&root, &fully_exposed, &[], 0b10);

        assert_eq!(
            free,
            vec![
                ExactCompleteTurnProposalSupportClaimV1::DebuffProtectionRemoval {
                    target_slot: 0,
                    remaining_stacks: 1,
                    potion_slot_mask: 0,
                }
            ]
        );
        assert_eq!(
            energy_spent,
            vec![
                ExactCompleteTurnProposalSupportClaimV1::DebuffProtectionRemoval {
                    target_slot: 0,
                    remaining_stacks: 1,
                    potion_slot_mask: 0b10,
                }
            ]
        );
        assert_eq!(
            completed,
            vec![
                ExactCompleteTurnProposalSupportClaimV1::DebuffProtectionRemoval {
                    target_slot: 0,
                    remaining_stacks: 0,
                    potion_slot_mask: 0b10,
                }
            ]
        );
    }

    #[test]
    fn durable_strength_gain_claims_separate_resource_domains() {
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
        let mut successor = root.clone();
        successor.combat.entities.power_db.insert(
            successor.combat.entities.player.id,
            vec![Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: 3,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        let action_facts = [strength_delta_fact("play_card", 3)];

        assert_eq!(
            support_claims(&root, &successor, &action_facts, 0),
            vec![
                ExactCompleteTurnProposalSupportClaimV1::DurablePlayerStrengthGain {
                    strength_gain: 3,
                    potion_slot_mask: 0,
                }
            ]
        );
        assert_eq!(
            support_claims(&root, &successor, &action_facts, 0b1),
            vec![
                ExactCompleteTurnProposalSupportClaimV1::DurablePlayerStrengthGain {
                    strength_gain: 3,
                    potion_slot_mask: 0b1,
                }
            ]
        );

        let end_turn_facts = [strength_delta_fact("end_turn", 3)];
        assert!(support_claims(&root, &successor, &end_turn_facts, 0).is_empty());
    }

    #[test]
    fn only_playing_a_power_card_claims_its_durable_installation() {
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
        let selected_for_exhaust = power_card_fact("submit_selection", CardId::DemonForm);
        assert!(support_claims(&root, &root, &[selected_for_exhaust], 0).is_empty());

        let played = power_card_fact("play_card", CardId::DemonForm);
        assert_eq!(
            support_claims(&root, &root, &[played], 0),
            vec![
                ExactCompleteTurnProposalSupportClaimV1::DurablePlayerPowerInstalled {
                    source_card_id: "DemonForm".to_owned(),
                    potion_slot_mask: 0,
                }
            ]
        );
    }

    #[test]
    fn prepared_draw_claim_requires_the_selected_card_to_remain_accessible() {
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
        let mut successor = root.clone();
        successor
            .combat
            .zones
            .hand
            .push(CombatCard::for_scenario(CardId::DoubleTap, 42));
        let mut fact = strength_delta_fact("submit_selection", 0);
        fact.pending_choice_selection = Some(CombatSearchV2ActionPendingChoiceSelectionFacts {
            role: CombatSearchV2ActionPendingChoiceSelectionRole::Value,
            primary: 0,
            secondary: 0,
            selected_count_tiebreak: -1,
            policy_log2_bias: 0,
            prepared_draw_card_lineage: Some(sts_core::test_support::scenario_lineage(42)),
        });

        assert_eq!(
            support_claims(&root, &successor, &[fact.clone()], 0),
            vec![
                ExactCompleteTurnProposalSupportClaimV1::PreparedDrawAccess {
                    card_lineage: sts_core::test_support::scenario_lineage(42),
                    potion_slot_mask: 0,
                }
            ]
        );

        successor.combat.zones.hand.clear();
        assert!(support_claims(&root, &successor, &[fact], 0).is_empty());
    }

    #[test]
    fn exact_turn_proposals_never_merge_effectless_elixir_cancel_back_in() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.entities.potions = vec![Some(Potion::new(PotionId::Elixir, 7))];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 11)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

        let batch = exact_complete_turn_proposals_v1(
            &root,
            ExactCompleteTurnProposalConfigV1 {
                max_inner_nodes: 1_024,
                max_partial_frontier_states: 256,
                max_end_states: 256,
                per_bucket_limit: 256,
                ..ExactCompleteTurnProposalConfigV1::default()
            },
        );
        let elixir_proposals = batch
            .candidates
            .iter()
            .filter(|proposal| {
                proposal.actions.iter().any(|action| {
                    matches!(
                        action.input,
                        ClientInput::UsePotion {
                            potion_index: 0,
                            ..
                        }
                    )
                })
            })
            .collect::<Vec<_>>();

        assert!(
            !elixir_proposals.is_empty(),
            "material Elixir proposals must remain available"
        );
        for proposal in elixir_proposals {
            assert!(
                proposal
                    .actions
                    .iter()
                    .all(|action| !matches!(action.input, ClientInput::Cancel)),
                "the exact-only cancel input must not be merged into automatic proposals"
            );
            assert!(
                proposal.actions.iter().any(|action| {
                    matches!(
                        &action.input,
                        ClientInput::SubmitSelection(resolution)
                            if !resolution.selected_card_lineages().is_empty()
                    )
                }),
                "effectless Elixir proposal actions={:?} boundary={:?}",
                proposal
                    .actions
                    .iter()
                    .map(|action| &action.input)
                    .collect::<Vec<_>>(),
                proposal.boundary
            );
        }
    }

    #[test]
    fn expired_caller_deadline_reaches_same_turn_enumeration() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 11)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

        let batch = exact_complete_turn_proposals_guided_with_deadline_v1(
            &root,
            ExactCompleteTurnProposalConfigV1::default(),
            Some(Instant::now()),
            &|_| Vec::new(),
        );

        assert_eq!(batch.nodes_expanded, 0);
        assert_eq!(batch.nodes_generated, 0);
        assert!(batch.candidates.is_empty());
    }

    #[test]
    fn preserving_a_withheld_block_card_has_a_recall_claim() {
        let mut combat = blank_test_combat();
        let mut holder = planned_monster(EnemyId::JawWorm, 1);
        holder.id = 17;
        combat.entities.monsters = vec![holder];
        combat
            .zones
            .limbo
            .push(CombatCard::for_scenario(CardId::SecondWind, 41));
        combat.entities.power_db.insert(
            17,
            vec![sts_core::content::powers::core::stasis::power_holding_card(
                sts_core::test_support::scenario_lineage(41),
            )],
        );
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

        assert_eq!(
            support_claims(&root, &root, &[], 0),
            vec![
                ExactCompleteTurnProposalSupportClaimV1::WithheldSurvivalResourcePreserved {
                    holder_slot: 0,
                    potion_slot_mask: 0,
                }
            ]
        );

        let mut released = root.clone();
        released.combat.entities.monsters[0].current_hp = 0;
        assert!(support_claims(&root, &released, &[], 0).is_empty());
    }
}
