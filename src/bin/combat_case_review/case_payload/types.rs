use serde::Serialize;
use sts_oracle_runtime::ai::boss_matchup::BossMatchupAcquisitionPressureV0;
use sts_oracle_runtime::ai::combat_search_v2::{
    CombatDeficitEvidenceReport, CombatLineLabReport, CombatSearchV2WitnessReplay,
    CombatTurnPoolOpeningReport,
};
use sts_oracle_runtime::ai::strategy::deck_strategic_deficit::DeckStrategicDeficit;
use sts_oracle_runtime::eval::combat_case::{
    CombatCaseCardSummary, CombatCaseCombatSummary, CombatCaseGap, CombatCasePathStep,
    CombatCaseRunSummary, CombatCaseSource,
};
use sts_oracle_runtime::eval::run_control::{
    AtomicCombatSearchTraceSummaryV2, CombatCaseAdjudicationProbeV1,
};

use super::super::awakened_one_evidence::{
    AwakenedOneFailureEvidenceFrame, AwakenedOnePathAuditV0, StaticBossMatchupAuditV0,
};
use super::super::boss_pressure_lens::BossPressureLensReport;
use super::super::champ_phase::ChampPhaseAudit;
use super::super::classification::CombatGapReviewClassification;
use super::super::counterfactual_hp::CounterfactualHpProbe;
use super::super::focus::{CombatReviewFocus, CombatReviewFocusPriorRerun};
use super::super::frozen_panel_lanes::FrozenPanelLaneReview;
use super::super::key_card_lifecycle::KeyCardLifecycleReport;
use super::super::power_setup_counterfactual::PowerSetupCounterfactualProbe;
use super::super::quality_lanes::CombatQualityLaneReview;
use super::super::search_types::SearchReview;
use super::super::strategic_feedback::CombatStrategicFeedbackReport;

#[derive(Serialize)]
pub(crate) struct CombatCaseReview {
    pub(super) schema: &'static str,
    pub(super) case_path: String,
    pub(super) source: CombatCaseSource,
    pub(super) gap: CombatCaseGap,
    pub(super) run: CombatCaseRunSummary,
    pub(super) combat: CombatCaseCombatSummary,
    pub(super) deck: Vec<CombatCaseCardSummary>,
    pub(super) static_strategic_deficit: DeckStrategicDeficit,
    pub(super) relics: Vec<String>,
    pub(super) potions: Vec<Option<String>>,
    pub(super) path_tail: Vec<CombatCasePathStep>,
    pub(super) saved_atomic_combat_search: Option<AtomicCombatSearchTraceSummaryV2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) adjudication_probe: Option<CombatCaseAdjudicationProbeV1>,
    pub(super) ladder: Vec<SearchReview>,
    pub(super) classification: CombatGapReviewClassification,
    pub(super) review_focus: Option<CombatReviewFocus>,
    pub(super) review_focus_replay: Option<CombatSearchV2WitnessReplay>,
    pub(super) review_focus_prior_rerun: Option<CombatReviewFocusPriorRerun>,
    pub(super) line_lab: Option<CombatLineLabReport>,
    pub(super) quality_lanes: Option<CombatQualityLaneReview>,
    pub(super) counterfactual_hp_probe: Option<CounterfactualHpProbe>,
    pub(super) combat_deficit_evidence: Option<CombatDeficitEvidenceReport>,
    pub(super) combat_strategic_feedback: Option<CombatStrategicFeedbackReport>,
    pub(super) static_boss_matchup_audit_v0: Option<StaticBossMatchupAuditV0>,
    pub(super) boss_matchup_acquisition_pressure_v0: Option<BossMatchupAcquisitionPressureV0>,
    pub(super) awakened_one_failure_evidence: Option<AwakenedOneFailureEvidenceFrame>,
    pub(super) awakened_one_path_audit_v0: Option<AwakenedOnePathAuditV0>,
    pub(super) awakened_opening_probe: Option<CombatTurnPoolOpeningReport>,
    pub(super) power_setup_counterfactual: Option<PowerSetupCounterfactualProbe>,
    pub(super) boss_pressure_lens: Option<BossPressureLensReport>,
    pub(super) frozen_panel_lanes: Option<FrozenPanelLaneReview>,
    pub(super) champ_phase_audit: Option<ChampPhaseAudit>,
    pub(super) key_card_lifecycle: Option<KeyCardLifecycleReport>,
}

pub(crate) struct CombatCaseReviewArtifacts {
    pub(crate) ladder: Vec<SearchReview>,
    pub(crate) classification: CombatGapReviewClassification,
    pub(crate) review_focus: Option<CombatReviewFocus>,
    pub(crate) review_focus_replay: Option<CombatSearchV2WitnessReplay>,
    pub(crate) review_focus_prior_rerun: Option<CombatReviewFocusPriorRerun>,
    pub(crate) line_lab: Option<CombatLineLabReport>,
    pub(crate) quality_lanes: Option<CombatQualityLaneReview>,
    pub(crate) counterfactual_hp_probe: Option<CounterfactualHpProbe>,
    pub(crate) combat_deficit_evidence: Option<CombatDeficitEvidenceReport>,
    pub(crate) static_boss_matchup_audit_v0: Option<StaticBossMatchupAuditV0>,
    pub(crate) boss_matchup_acquisition_pressure_v0: Option<BossMatchupAcquisitionPressureV0>,
    pub(crate) awakened_one_failure_evidence: Option<AwakenedOneFailureEvidenceFrame>,
    pub(crate) awakened_one_path_audit_v0: Option<AwakenedOnePathAuditV0>,
    pub(crate) awakened_opening_probe: Option<CombatTurnPoolOpeningReport>,
    pub(crate) power_setup_counterfactual: Option<PowerSetupCounterfactualProbe>,
    pub(crate) boss_pressure_lens: Option<BossPressureLensReport>,
    pub(crate) frozen_panel_lanes: Option<FrozenPanelLaneReview>,
    pub(crate) champ_phase_audit: Option<ChampPhaseAudit>,
    pub(crate) adjudication_probe: Option<CombatCaseAdjudicationProbeV1>,
}
