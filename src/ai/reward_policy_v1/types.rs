use crate::ai::noncombat_decision_v1::{
    CandidateDescriptorV1, DataRoleV1, DecisionSiteKindV1, EvidenceBundleV1, EvidenceItemV1,
    EvidenceKindV1, InformationBoundaryV1, InformationClassV1, NonCombatDecisionRecordV1,
    PolicyProvenanceV1, PolicySelectionStatusV1, PolicySelectionV1, PublicActionPlanV1,
    NONCOMBAT_DECISION_RECORD_SCHEMA_NAME, NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
};
use crate::ai::noncombat_strategy_v1::StrategyPlanSupportV1;
use crate::content::potions::PotionId;
use crate::state::rewards::RewardItem;

#[derive(Clone, Debug, PartialEq)]
pub struct RewardDecisionContextV1 {
    pub pending_card_choice_open: bool,
    pub has_empty_potion_slot: bool,
    pub has_sozu: bool,
    pub has_sapphire_key_reward: bool,
    pub candidates: Vec<RewardCandidateEvidenceV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RewardCandidateEvidenceV1 {
    pub index: usize,
    pub candidate_id: String,
    pub label: String,
    pub potion_id: Option<PotionId>,
    pub class: RewardPolicyClassV1,
    pub support_gate: StrategyPlanSupportV1,
    pub evidence: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RewardPolicyClassV1 {
    Gold,
    StolenGold,
    PotionWithEmptySlot,
    PotionNoEmptySlot,
    PotionBlockedBySozu,
    RelicWithoutSapphireKeyConflict,
    RelicWithSapphireKeyConflict,
    CardReward,
    EmeraldKey,
    SapphireKey,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RewardPolicyConfigV1 {
    pub claim_gold: bool,
    pub claim_potion_with_empty_slot: bool,
    pub claim_safe_relic_without_sapphire_key: bool,
}

impl Default for RewardPolicyConfigV1 {
    fn default() -> Self {
        Self {
            claim_gold: true,
            claim_potion_with_empty_slot: true,
            claim_safe_relic_without_sapphire_key: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RewardDecisionV1 {
    pub action: RewardPolicyActionV1,
    pub label_role: &'static str,
    pub context: RewardDecisionContextV1,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RewardPolicyActionV1 {
    Claim {
        index: usize,
        label: String,
        confidence: f32,
        reason: String,
    },
    Stop {
        reason: String,
    },
}

impl RewardDecisionV1 {
    pub fn to_noncombat_decision_record_v1(&self) -> NonCombatDecisionRecordV1 {
        let selected_candidate_id = match &self.action {
            RewardPolicyActionV1::Claim { index, .. } => self
                .context
                .candidates
                .iter()
                .find(|candidate| candidate.index == *index)
                .map(|candidate| candidate.candidate_id.clone()),
            RewardPolicyActionV1::Stop { .. } => None,
        };

        NonCombatDecisionRecordV1 {
            schema_name: NONCOMBAT_DECISION_RECORD_SCHEMA_NAME.to_string(),
            schema_version: NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
            site: DecisionSiteKindV1::Reward,
            data_role: DataRoleV1::BehaviorPolicyNotTeacher,
            information_boundary: InformationBoundaryV1::hidden_free(vec![
                InformationClassV1::PublicObservation,
                InformationClassV1::Belief,
            ]),
            provenance: PolicyProvenanceV1 {
                source_policy: "reward_policy_v1".to_string(),
                source_schema_name: "RewardDecisionV1".to_string(),
                source_schema_version: 1,
            },
            candidates: self
                .context
                .candidates
                .iter()
                .map(candidate_descriptor)
                .collect(),
            evidence: EvidenceBundleV1 {
                items: evidence_items(&self.context),
                assumptions: vec![
                    "reward automation only claims low-agency public rewards".to_string(),
                    "ordinary relic rewards are auto-claimed only when no Sapphire Key is present on the same reward screen".to_string(),
                    "reward automation is a behavior policy, not a teacher label".to_string(),
                ],
                warnings: Vec::new(),
            },
            values: Vec::new(),
            selection: match &self.action {
                RewardPolicyActionV1::Claim {
                    confidence, reason, ..
                } => PolicySelectionV1 {
                    status: PolicySelectionStatusV1::Selected,
                    selected_candidate_id,
                    reason: reason.clone(),
                    confidence: *confidence,
                    selection_mode: "low_agency_reward_claim_v1".to_string(),
                },
                RewardPolicyActionV1::Stop { reason } => PolicySelectionV1 {
                    status: PolicySelectionStatusV1::Stopped,
                    selected_candidate_id,
                    reason: reason.clone(),
                    confidence: 0.0,
                    selection_mode: "human_required".to_string(),
                },
            },
        }
    }
}

fn candidate_descriptor(candidate: &RewardCandidateEvidenceV1) -> CandidateDescriptorV1 {
    CandidateDescriptorV1 {
        candidate_id: candidate.candidate_id.clone(),
        site: DecisionSiteKindV1::Reward,
        label: candidate.label.clone(),
        action_plan: PublicActionPlanV1 {
            summary: format!("claim reward index {}", candidate.index),
            command: Some(format!("claim {}", candidate.index)),
        },
        information_classes: vec![InformationClassV1::PublicObservation],
        uncertainty_notes: candidate.risks.clone(),
    }
}

fn evidence_items(context: &RewardDecisionContextV1) -> Vec<EvidenceItemV1> {
    context
        .candidates
        .iter()
        .map(|candidate| EvidenceItemV1 {
            kind: EvidenceKindV1::CandidateFacts,
            candidate_id: Some(candidate.candidate_id.clone()),
            label: format!(
                "{}: {:?} gate={:?}",
                candidate.label, candidate.class, candidate.support_gate
            ),
            information_class: InformationClassV1::PublicObservation,
            components: Vec::new(),
        })
        .collect()
}

pub(crate) fn reward_candidate_id(index: usize, item: &RewardItem) -> String {
    match item {
        RewardItem::Gold { .. } => format!("reward:gold:{index}"),
        RewardItem::StolenGold { .. } => format!("reward:stolen_gold:{index}"),
        RewardItem::Card { .. } => format!("reward:card:{index}"),
        RewardItem::Relic { relic_id } => {
            format!("reward:relic:{index}:{}", relic_id.stable_id())
        }
        RewardItem::Potion { potion_id } => {
            format!("reward:potion:{index}:{}", potion_id.stable_id())
        }
        RewardItem::EmeraldKey => format!("reward:emerald_key:{index}"),
        RewardItem::SapphireKey => format!("reward:sapphire_key:{index}"),
    }
}
