use std::collections::BTreeSet;

use serde::Serialize;

use crate::content::potions::Potion;
use crate::content::relics::RelicState;
use crate::runtime::combat::CombatCard;

use super::{
    build_decision_surface, ExactCardRewardPolicyAuditV1, OracleAnalysisSessionV1,
    OracleRunBoundaryV1,
};
use crate::eval::run_control::DecisionCandidateKey;

pub const ORACLE_ANALYSIS_CARD_REWARD_PATH_AUDIT_SCHEMA_NAME: &str =
    "OracleAnalysisCardRewardPathAudit";
pub const ORACLE_ANALYSIS_CARD_REWARD_PATH_AUDIT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnalysisCardRewardApplicationUnknownV1 {
    MissingEdge,
    AmbiguousEdge,
    MissingChoiceRef,
    ChoiceNotRetained,
    CandidateNotInCurrentAudit,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum OracleAnalysisCardRewardApplicationV1 {
    Uncommitted,
    Applied {
        edge_id: u64,
        child_node_id: usize,
        choice_ref: String,
        candidate_id: String,
        materialized_owner_rank: u64,
        current_owner_rank: usize,
        owner_rank_changed: bool,
    },
    Unknown {
        child_node_id: usize,
        reason: OracleAnalysisCardRewardApplicationUnknownV1,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCardRewardPathBoundaryV1 {
    pub node_id: usize,
    pub state_fingerprint: String,
    pub act: u8,
    pub floor: i32,
    pub deck: Vec<CombatCard>,
    pub relics: Vec<RelicState>,
    pub potions: Vec<Option<Potion>>,
    pub application: OracleAnalysisCardRewardApplicationV1,
    pub audit: ExactCardRewardPolicyAuditV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisCardRewardPathAuditV1 {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub target_node_id: usize,
    pub boundaries: Vec<OracleAnalysisCardRewardPathBoundaryV1>,
}

impl OracleAnalysisSessionV1 {
    /// Audit every exact card-reward surface on one retained node's canonical
    /// lineage in a single read-only traversal.
    ///
    /// The report keeps candidate keys and owner evidence typed, joins the
    /// applied historical edge to the freshly recomputed owner rank, and
    /// carries the exact deck/relic/potion state at each boundary. Callers do
    /// not need to inspect the workspace checkpoint or guess reward node ids.
    pub fn card_reward_path_audit(
        &self,
        target_node_id: usize,
    ) -> Result<OracleAnalysisCardRewardPathAuditV1, String> {
        let lineage = self.canonical_lineage_node_ids(target_node_id)?;
        let mut boundaries = Vec::new();

        for (index, node_id) in lineage.iter().copied().enumerate() {
            let branch = self.require_branch(node_id)?;
            if branch.boundary != OracleRunBoundaryV1::Reward {
                continue;
            }

            let surface = build_decision_surface(&branch.session);
            let is_card_reward_surface = surface.view.candidates.iter().any(|candidate| {
                matches!(
                    candidate.key.as_ref(),
                    Some(
                        DecisionCandidateKey::CardRewardPick { .. }
                            | DecisionCandidateKey::CardRewardSingingBowl { .. }
                    )
                )
            });
            if !is_card_reward_surface {
                continue;
            }

            let audit = self.card_reward_policy_audit(node_id)?;
            let next_node_id = lineage.get(index + 1).copied();
            let application = self.card_reward_path_application(node_id, next_node_id, &audit)?;
            let run = &branch.session.run_state;
            boundaries.push(OracleAnalysisCardRewardPathBoundaryV1 {
                node_id,
                state_fingerprint: branch.state_fingerprint.clone(),
                act: run.act_num,
                floor: run.floor_num,
                deck: run.master_deck.clone(),
                relics: run.relics.clone(),
                potions: run.potions.clone(),
                application,
                audit,
            });
        }

        Ok(OracleAnalysisCardRewardPathAuditV1 {
            schema_name: ORACLE_ANALYSIS_CARD_REWARD_PATH_AUDIT_SCHEMA_NAME,
            schema_version: ORACLE_ANALYSIS_CARD_REWARD_PATH_AUDIT_SCHEMA_VERSION,
            target_node_id,
            boundaries,
        })
    }

    fn canonical_lineage_node_ids(&self, target_node_id: usize) -> Result<Vec<usize>, String> {
        self.require_branch(target_node_id)?;
        let mut lineage = Vec::new();
        let mut visited = BTreeSet::new();
        let mut current = target_node_id;
        loop {
            if !visited.insert(current) {
                return Err(format!(
                    "oracle analysis canonical lineage contains a cycle at node {current}"
                ));
            }
            lineage.push(current);
            let branch = self.require_branch(current)?;
            let Some(parent) = branch.parent_branch_id else {
                break;
            };
            if self.require_branch(parent).is_err() {
                break;
            }
            current = parent;
        }
        lineage.reverse();
        Ok(lineage)
    }

    fn card_reward_path_application(
        &self,
        node_id: usize,
        next_node_id: Option<usize>,
        audit: &ExactCardRewardPolicyAuditV1,
    ) -> Result<OracleAnalysisCardRewardApplicationV1, String> {
        let Some(child_node_id) = next_node_id else {
            return Ok(OracleAnalysisCardRewardApplicationV1::Uncommitted);
        };
        let matching_edges = self
            .edges
            .iter()
            .filter(|edge| edge.parent_node_id == node_id && edge.child_node_id == child_node_id)
            .collect::<Vec<_>>();
        let [edge] = matching_edges.as_slice() else {
            let reason = if matching_edges.is_empty() {
                OracleAnalysisCardRewardApplicationUnknownV1::MissingEdge
            } else {
                OracleAnalysisCardRewardApplicationUnknownV1::AmbiguousEdge
            };
            return Ok(OracleAnalysisCardRewardApplicationV1::Unknown {
                child_node_id,
                reason,
            });
        };
        let Some(choice_ref) = edge.choice_ref.as_ref() else {
            return Ok(OracleAnalysisCardRewardApplicationV1::Unknown {
                child_node_id,
                reason: OracleAnalysisCardRewardApplicationUnknownV1::MissingChoiceRef,
            });
        };
        let view = self.view_node(node_id)?;
        let Some(choice) = view
            .choices
            .iter()
            .find(|choice| choice.choice_ref == *choice_ref)
        else {
            return Ok(OracleAnalysisCardRewardApplicationV1::Unknown {
                child_node_id,
                reason: OracleAnalysisCardRewardApplicationUnknownV1::ChoiceNotRetained,
            });
        };
        let Some(current) = audit
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id == choice.candidate_id)
        else {
            return Ok(OracleAnalysisCardRewardApplicationV1::Unknown {
                child_node_id,
                reason: OracleAnalysisCardRewardApplicationUnknownV1::CandidateNotInCurrentAudit,
            });
        };

        Ok(OracleAnalysisCardRewardApplicationV1::Applied {
            edge_id: edge.edge_id,
            child_node_id,
            choice_ref: choice_ref.clone(),
            candidate_id: choice.candidate_id.clone(),
            materialized_owner_rank: choice.owner_rank,
            current_owner_rank: current.owner_rank,
            owner_rank_changed: usize::try_from(choice.owner_rank) != Ok(current.owner_rank),
        })
    }
}
