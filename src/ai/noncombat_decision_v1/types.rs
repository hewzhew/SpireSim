use serde::{Deserialize, Serialize};

pub const NONCOMBAT_DECISION_RECORD_SCHEMA_NAME: &str = "NonCombatDecisionRecordV1";
pub const NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DecisionSiteKindV1 {
    Map,
    CardReward,
    Neow,
    Event,
    Shop,
    Campfire,
    BossRelic,
    Reward,
    RunChoice,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DataRoleV1 {
    BehaviorPolicyNotTeacher,
    HumanBoundaryNotTeacher,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum InformationClassV1 {
    PublicObservation,
    KnownDistribution,
    Belief,
    HiddenSimulatorState,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InformationBoundaryV1 {
    pub allowed_inputs: Vec<InformationClassV1>,
    pub forbidden_inputs: Vec<InformationClassV1>,
    pub hidden_simulator_state_used: bool,
}

impl InformationBoundaryV1 {
    pub fn hidden_free(allowed_inputs: Vec<InformationClassV1>) -> Self {
        Self {
            allowed_inputs,
            forbidden_inputs: vec![InformationClassV1::HiddenSimulatorState],
            hidden_simulator_state_used: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NonCombatDecisionRecordV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub site: DecisionSiteKindV1,
    pub data_role: DataRoleV1,
    pub information_boundary: InformationBoundaryV1,
    pub provenance: PolicyProvenanceV1,
    pub candidates: Vec<CandidateDescriptorV1>,
    pub evidence: EvidenceBundleV1,
    pub values: Vec<ValueEstimateV1>,
    pub selection: PolicySelectionV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PolicyProvenanceV1 {
    pub source_policy: String,
    pub source_schema_name: String,
    pub source_schema_version: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CandidateDescriptorV1 {
    pub candidate_id: String,
    pub site: DecisionSiteKindV1,
    pub label: String,
    pub action_plan: PublicActionPlanV1,
    pub information_classes: Vec<InformationClassV1>,
    pub uncertainty_notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PublicActionPlanV1 {
    pub summary: String,
    pub command: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EvidenceBundleV1 {
    pub items: Vec<EvidenceItemV1>,
    pub assumptions: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EvidenceItemV1 {
    pub kind: EvidenceKindV1,
    pub candidate_id: Option<String>,
    pub label: String,
    pub information_class: InformationClassV1,
    pub components: Vec<ValueComponentV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EvidenceKindV1 {
    CandidateFacts,
    NeedVector,
    ValueFactors,
    ScoreTerms,
    PolicyGate,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ValueEstimateV1 {
    pub candidate_id: String,
    pub mean_utility: f32,
    pub risk_adjusted_utility: f32,
    pub confidence: f32,
    pub components: Vec<ValueComponentV1>,
    pub evidence_refs: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ValueComponentV1 {
    pub name: String,
    pub value: f32,
}

impl ValueComponentV1 {
    pub fn new(name: impl Into<String>, value: f32) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PolicySelectionV1 {
    pub status: PolicySelectionStatusV1,
    pub selected_candidate_id: Option<String>,
    pub reason: String,
    pub confidence: f32,
    pub selection_mode: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PolicySelectionStatusV1 {
    Selected,
    Stopped,
    NoCandidates,
}
