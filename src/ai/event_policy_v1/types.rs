use crate::ai::noncombat_decision_v1::{
    CandidateDescriptorV1, DataRoleV1, DecisionSiteKindV1, EvidenceBundleV1, EvidenceItemV1,
    EvidenceKindV1, InformationBoundaryV1, InformationClassV1, NonCombatDecisionRecordV1,
    PolicyProvenanceV1, PolicySelectionStatusV1, PolicySelectionV1, PublicActionPlanV1,
    NONCOMBAT_DECISION_RECORD_SCHEMA_NAME, NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
};
use crate::ai::noncombat_strategy_v1::{
    RunStrategySnapshotV2, StrategyPackageIdV2, StrategyPlanSupportV1,
};
use crate::ai::random_upgrade_opportunity_v1::RandomUpgradeOpportunityPlanV1;
use crate::state::events::EventId;

#[derive(Clone, Debug, PartialEq)]
pub struct EventDecisionContextV1 {
    pub event_id: EventId,
    pub strategy: RunStrategySnapshotV2,
    pub current_hp: i32,
    pub max_hp: i32,
    pub has_mark_of_the_bloom: bool,
    pub candidates: Vec<EventCandidateEvidenceV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventCandidateEvidenceV1 {
    pub index: usize,
    pub label: String,
    pub class: EventPolicyClassV1,
    pub evaluation: EventCandidateEvaluationV1,
    pub support_gate: StrategyPlanSupportV1,
    pub evidence: Vec<String>,
    pub risks: Vec<String>,
    pub disabled: bool,
    pub hp_cost: i32,
    pub max_hp_loss: i32,
    pub heal_amount: i32,
    pub max_hp_gain: i32,
    pub curse_count: i32,
    pub obtained_card_count: i32,
    pub obtains_mark_of_the_bloom: bool,
    pub random_upgrade_opportunity: Option<RandomUpgradeOpportunityPlanV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventPolicyClassV1 {
    FreeKnownBenefit,
    SafeExit,
    MaxHpForHpCost,
    ResourceCost,
    RandomUpgradeOpportunity,
    CurseDebt,
    SelectionOrDeckMutation,
    CombatStart,
    UncertainReward,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventCandidateEvaluationV1 {
    pub score: i32,
    pub tier: EventCandidateTierV1,
    pub reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EventCandidateTierV1 {
    Preferred,
    Viable,
    Risky,
    Avoid,
    Blocked,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventPolicyConfigV1 {
    pub allow_free_known_benefit: bool,
    pub allow_safe_exit_from_risky_event: bool,
    pub allow_max_hp_for_safe_hp_cost: bool,
    pub allow_random_upgrade_opportunity: bool,
    pub min_hp_after_safe_hp_cost: i32,
    pub min_hp_ratio_after_safe_hp_cost: f32,
}

impl Default for EventPolicyConfigV1 {
    fn default() -> Self {
        Self {
            allow_free_known_benefit: true,
            allow_safe_exit_from_risky_event: true,
            allow_max_hp_for_safe_hp_cost: true,
            allow_random_upgrade_opportunity: true,
            min_hp_after_safe_hp_cost: 35,
            min_hp_ratio_after_safe_hp_cost: 0.50,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventDecisionV1 {
    pub action: EventPolicyActionV1,
    pub label_role: &'static str,
    pub context: EventDecisionContextV1,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventPolicyActionV1 {
    Pick {
        index: usize,
        label: String,
        confidence: f32,
        reason: String,
    },
    Stop {
        reason: String,
    },
}

impl EventDecisionV1 {
    pub fn to_noncombat_decision_record_v1(&self) -> NonCombatDecisionRecordV1 {
        let selected_candidate_id = match &self.action {
            EventPolicyActionV1::Pick { index, .. } => Some(candidate_id(*index)),
            EventPolicyActionV1::Stop { .. } => None,
        };

        NonCombatDecisionRecordV1 {
            schema_name: NONCOMBAT_DECISION_RECORD_SCHEMA_NAME.to_string(),
            schema_version: NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
            site: DecisionSiteKindV1::Event,
            data_role: DataRoleV1::BehaviorPolicyNotTeacher,
            information_boundary: InformationBoundaryV1::hidden_free(vec![
                InformationClassV1::PublicObservation,
                InformationClassV1::Belief,
            ]),
            provenance: PolicyProvenanceV1 {
                source_policy: "event_policy_v1".to_string(),
                source_schema_name: "EventDecisionV1".to_string(),
                source_schema_version: 1,
            },
            candidates: self.context.candidates.iter().map(candidate_descriptor).collect(),
            evidence: EvidenceBundleV1 {
                items: evidence_items(&self.context),
                assumptions: vec![
                    "event automation only uses structured public event semantics and V2 strategy packages"
                        .to_string(),
                    "event automation is a behavior policy, not a teacher label".to_string(),
                ],
                warnings: Vec::new(),
            },
            values: Vec::new(),
            selection: match &self.action {
                EventPolicyActionV1::Pick {
                    confidence,
                    reason,
                    ..
                } => PolicySelectionV1 {
                    status: PolicySelectionStatusV1::Selected,
                    selected_candidate_id,
                    reason: reason.clone(),
                    confidence: *confidence,
                    selection_mode: "event_autopilot_pick_v1".to_string(),
                },
                EventPolicyActionV1::Stop { reason } => PolicySelectionV1 {
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

fn candidate_descriptor(candidate: &EventCandidateEvidenceV1) -> CandidateDescriptorV1 {
    CandidateDescriptorV1 {
        candidate_id: candidate_id(candidate.index),
        site: DecisionSiteKindV1::Event,
        label: candidate.label.clone(),
        action_plan: PublicActionPlanV1 {
            summary: format!("choose event option {}", candidate.index),
            command: Some(format!("event {}", candidate.index)),
        },
        information_classes: vec![InformationClassV1::PublicObservation],
        uncertainty_notes: candidate.risks.clone(),
    }
}

fn evidence_items(context: &EventDecisionContextV1) -> Vec<EvidenceItemV1> {
    let mut items = context
        .candidates
        .iter()
        .map(|candidate| EvidenceItemV1 {
            kind: EvidenceKindV1::CandidateFacts,
            candidate_id: Some(candidate_id(candidate.index)),
            label: format!(
                "{}: {:?} gate={:?}",
                candidate.label, candidate.class, candidate.support_gate
            ),
            information_class: InformationClassV1::PublicObservation,
            components: Vec::new(),
        })
        .collect::<Vec<_>>();

    for id in [
        StrategyPackageIdV2::HpSafety,
        StrategyPackageIdV2::GoldPlan,
        StrategyPackageIdV2::ShopRemoveWindow,
        StrategyPackageIdV2::CorePlanProtection,
        StrategyPackageIdV2::RelicConstraints,
    ] {
        if let Some(package) = context.strategy.package(id) {
            items.push(EvidenceItemV1 {
                kind: EvidenceKindV1::PolicyGate,
                candidate_id: None,
                label: format!("strategy package: {:?}/{:?}", package.domain, package.id),
                information_class: InformationClassV1::Belief,
                components: Vec::new(),
            });
        }
    }
    items
}

fn candidate_id(index: usize) -> String {
    format!("event:{index}")
}
