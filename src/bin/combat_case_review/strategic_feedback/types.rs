use serde::Serialize;
use sts_oracle_runtime::ai::combat_search_v2::SearchTerminalLabel;
use sts_oracle_runtime::ai::strategy::deck_strategic_deficit::{
    StrategicBurdenLevel, StrategicDeficitLevel,
};

#[derive(Serialize)]
pub(crate) struct CombatStrategicFeedbackReport {
    pub(super) schema: &'static str,
    pub(super) site: CombatStrategicSite,
    pub(super) signals: Vec<CombatStrategicSignal>,
    pub(super) observations: CombatStrategicFeedbackObservations,
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CombatStrategicSite {
    ActBoss,
    EliteLike,
    HallwayOrUnknown,
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CombatStrategicSignal {
    SearchExecutionGap,
    ActBossNoWinAfterReview,
    Act2BossNoWinAfterReview,
    LowHpAtCombatStart,
    LowHpReachedAct3Elite,
    ScalingMissingForBoss,
    ScalingThinUnderBossPressure,
    FrontloadSurplusButBossNoWin,
    StaticBlockAdequateButFatalLoss,
    StaticAoeAdequateButMultiEnemyNoWin,
    StaticScalingAdequateButNoWin,
}

#[derive(Serialize)]
pub(super) struct CombatStrategicFeedbackObservations {
    pub(super) review_kind: &'static str,
    pub(super) focus_source: Option<&'static str>,
    pub(super) focus_terminal: Option<SearchTerminalLabel>,
    pub(super) focus_estimated: Option<bool>,
    pub(super) focus_final_hp: Option<i32>,
    pub(super) focus_hp_loss: Option<i32>,
    pub(super) focus_living_enemy_count: Option<usize>,
    pub(super) focus_total_enemy_hp: Option<i32>,
    pub(super) enemy_count: usize,
    pub(super) hp_ratio_pct: i32,
    pub(super) static_frontload: StrategicDeficitLevel,
    pub(super) static_aoe: StrategicDeficitLevel,
    pub(super) static_block: StrategicDeficitLevel,
    pub(super) static_scaling: StrategicDeficitLevel,
    pub(super) static_burden: StrategicBurdenLevel,
}
