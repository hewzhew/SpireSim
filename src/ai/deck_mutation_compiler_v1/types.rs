use serde::Serialize;

use crate::content::cards::CardId;
use crate::runtime::combat::CardLineageId;
use crate::state::core::RunPendingChoiceReason;

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledDeckMutationDecisionV1 {
    pub reason: RunPendingChoiceReason,
    pub min_choices: usize,
    pub max_choices: usize,
    pub output: DeckMutationCompilerOutputV1,
    pub commitment: DeckMutationCommitmentModeV1,
    pub selected_plan: Option<DeckMutationPlanCandidateV1>,
    pub branch_active_plans: Vec<DeckMutationPlanCandidateV1>,
    pub inspect_only_plans: Vec<DeckMutationPlanCandidateV1>,
    pub blocked_plans: Vec<DeckMutationPlanCandidateV1>,
    pub candidate_plans: Vec<DeckMutationPlanCandidateV1>,
    pub label_role: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeckMutationCompilerRequestV1 {
    pub output: DeckMutationCompilerOutputV1,
    pub commitment: DeckMutationCommitmentModeV1,
}

impl DeckMutationCompilerRequestV1 {
    pub fn optional_execute_one() -> Self {
        Self {
            output: DeckMutationCompilerOutputV1::ExecuteOne,
            commitment: DeckMutationCommitmentModeV1::Optional,
        }
    }

    pub fn committed_forced_execute_one() -> Self {
        Self {
            output: DeckMutationCompilerOutputV1::ExecuteOne,
            commitment: DeckMutationCommitmentModeV1::CommittedForced,
        }
    }

    pub fn optional_branch_top_k(max_active: usize) -> Self {
        Self {
            output: DeckMutationCompilerOutputV1::BranchTopK { max_active },
            commitment: DeckMutationCommitmentModeV1::Optional,
        }
    }

    pub fn optional_inspect() -> Self {
        Self {
            output: DeckMutationCompilerOutputV1::Inspect,
            commitment: DeckMutationCommitmentModeV1::Optional,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeckMutationCompilerOutputV1 {
    ExecuteOne,
    BranchTopK { max_active: usize },
    Inspect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeckMutationCommitmentModeV1 {
    Optional,
    CommittedForced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeckMutationKindV1 {
    Remove,
    Upgrade,
    Transform,
    Duplicate,
    Bottle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateTargetRoleV1 {
    SetupAccelerator,
    EnginePayoff,
    EngineEnabler,
    CompactBossAnswer,
    WinCondition,
    OrdinaryFiller,
    Reject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateStackBehaviorV1 {
    Stackable,
    ConsistencyOnly,
    NonStackingDeadAfterFirst,
    Ordinary,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DuplicateTargetEvaluationV1 {
    pub card: CardId,
    pub upgrades: u8,
    pub priority: i32,
    pub premium: bool,
    pub role: DuplicateTargetRoleV1,
    pub stack_behavior: DuplicateStackBehaviorV1,
    pub reasons: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DeckMutationPlanRoleV1 {
    PolicyPreferred,
    SafeAlternative,
    RiskyExploration,
    InspectOnly,
    Blocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeckMutationTargetClassV1 {
    Curse,
    StarterStrike,
    StarterDefend,
    Basic,
    BasicCore,
    Functional,
    UpgradeTarget,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum DeckMutationTargetLossTierV1 {
    #[default]
    LowValue,
    RedundantFunctional,
    Functional,
    CoreFunctional,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum DeckMutationRemovalBenefitTierV1 {
    #[default]
    None,
    LowValueCard,
    DrawObstruction,
    PersistentDrawObstruction,
    ForcedOpeningObstruction,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeckMutationRemovalBenefitV1 {
    pub tier: DeckMutationRemovalBenefitTierV1,
    pub signals: Vec<String>,
    pub compensating_signals: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeckMutationTargetLossV1 {
    pub tier: DeckMutationTargetLossTierV1,
    pub same_card_count: usize,
    pub signals: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum DeckMutationOpeningHandDebtTierV1 {
    #[default]
    None,
    Mild,
    Situational,
    High,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeckMutationOpeningHandProfileV1 {
    pub debt_tier: DeckMutationOpeningHandDebtTierV1,
    pub score_hint: i32,
    pub signals: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum TransformRandomAdditionBandV1 {
    #[default]
    NotTransform,
    LikelyBetterThanTarget,
    Mixed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum TransformVarianceRiskV1 {
    #[default]
    NotTransform,
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckMutationTransformProfileV1 {
    pub random_addition_band: TransformRandomAdditionBandV1,
    pub variance_risk: TransformVarianceRiskV1,
    pub branch_lane: DeckMutationPlanRoleV1,
    pub signals: Vec<String>,
}

impl Default for DeckMutationTransformProfileV1 {
    fn default() -> Self {
        Self {
            random_addition_band: TransformRandomAdditionBandV1::NotTransform,
            variance_risk: TransformVarianceRiskV1::NotTransform,
            branch_lane: DeckMutationPlanRoleV1::InspectOnly,
            signals: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AllowedDeckMutationConsumersV1 {
    pub execute_autopilot: bool,
    pub branch_active: bool,
    pub branch_frozen: bool,
    pub inspect: bool,
    pub replay: bool,
    pub human_prompt: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckMutationCardSnapshotV1 {
    pub deck_index: usize,
    pub lineage: CardLineageId,
    pub card: CardId,
    pub upgrades: u8,
    pub label: String,
    pub target_class: DeckMutationTargetClassV1,
    pub removal_benefit: DeckMutationRemovalBenefitV1,
    pub target_loss: DeckMutationTargetLossV1,
    pub opening_hand: DeckMutationOpeningHandProfileV1,
    pub transform: DeckMutationTransformProfileV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckMutationPlanStepV1 {
    pub kind: DeckMutationKindV1,
    pub deck_indices: Vec<usize>,
    pub cards: Vec<DeckMutationCardSnapshotV1>,
    pub command: String,
    pub effect_kind: String,
    pub effect_key: String,
    pub effect_label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckMutationPlanCandidateV1 {
    pub plan_id: String,
    pub step: DeckMutationPlanStepV1,
    pub role: DeckMutationPlanRoleV1,
    pub allowed_consumers: AllowedDeckMutationConsumersV1,
    pub representative_count: usize,
    pub suppressed_count: usize,
    pub score_hint: i32,
    pub confidence: f32,
    pub duplicate_targets: Vec<DuplicateTargetEvaluationV1>,
    pub reasons: Vec<String>,
    pub risks: Vec<String>,
}

impl DeckMutationPlanCandidateV1 {
    pub fn deck_indices(&self) -> &[usize] {
        &self.step.deck_indices
    }
}
