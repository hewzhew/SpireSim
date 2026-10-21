use sts_oracle_runtime::ai::strategy::deck_strategic_deficit::{
    DeckStrategicDeficit, StrategicDeficitLevel,
};
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::super::classification::CombatGapReviewClassification;
use super::super::search_types::{SearchDiagnosticProgressFacts, SearchReview};
use super::signal_context::CombatStrategicSignalContext;
use super::types::{CombatStrategicSignal, CombatStrategicSite};

pub(super) fn strategic_signals(
    case: &CombatCase,
    static_deficit: &DeckStrategicDeficit,
    classification: &CombatGapReviewClassification,
    progress: Option<&SearchDiagnosticProgressFacts>,
    site: CombatStrategicSite,
    ladder: &[SearchReview],
) -> Vec<CombatStrategicSignal> {
    let context = CombatStrategicSignalContext::new(case, classification, progress, ladder);

    let mut signals = Vec::new();
    if context.no_exact_win && context.rollout_win {
        push_signal(&mut signals, CombatStrategicSignal::SearchExecutionGap);
    }
    if context.no_win_after_review && site == CombatStrategicSite::ActBoss {
        push_signal(&mut signals, CombatStrategicSignal::ActBossNoWinAfterReview);
        if case.core.run.act == 2 {
            push_signal(
                &mut signals,
                CombatStrategicSignal::Act2BossNoWinAfterReview,
            );
        }
    }
    if context.no_win_after_review && context.low_hp_start {
        push_signal(&mut signals, CombatStrategicSignal::LowHpAtCombatStart);
    }
    if context.no_win_after_review
        && case.core.run.act >= 3
        && site == CombatStrategicSite::EliteLike
        && context.low_hp_start
    {
        push_signal(&mut signals, CombatStrategicSignal::LowHpReachedAct3Elite);
    }
    if context.no_win_after_review && site == CombatStrategicSite::ActBoss {
        match static_deficit.boss_scaling_plan {
            StrategicDeficitLevel::Missing => {
                push_signal(&mut signals, CombatStrategicSignal::ScalingMissingForBoss);
            }
            StrategicDeficitLevel::Thin => {
                push_signal(
                    &mut signals,
                    CombatStrategicSignal::ScalingThinUnderBossPressure,
                );
            }
            StrategicDeficitLevel::Adequate | StrategicDeficitLevel::Surplus => {
                push_signal(
                    &mut signals,
                    CombatStrategicSignal::StaticScalingAdequateButNoWin,
                );
            }
        }
        if static_deficit.frontload_damage == StrategicDeficitLevel::Surplus {
            push_signal(
                &mut signals,
                CombatStrategicSignal::FrontloadSurplusButBossNoWin,
            );
        }
    }
    if context.no_win_after_review
        && context.exact_loss
        && !context.low_hp_start
        && matches!(
            static_deficit.block_or_mitigation,
            StrategicDeficitLevel::Adequate | StrategicDeficitLevel::Surplus
        )
    {
        push_signal(
            &mut signals,
            CombatStrategicSignal::StaticBlockAdequateButFatalLoss,
        );
    }
    if context.no_win_after_review
        && context.exact_loss
        && case.core.combat.enemies.len() > 1
        && matches!(
            static_deficit.aoe_or_minion_control,
            StrategicDeficitLevel::Adequate | StrategicDeficitLevel::Surplus
        )
    {
        push_signal(
            &mut signals,
            CombatStrategicSignal::StaticAoeAdequateButMultiEnemyNoWin,
        );
    }
    signals
}

fn push_signal(signals: &mut Vec<CombatStrategicSignal>, signal: CombatStrategicSignal) {
    if !signals.contains(&signal) {
        signals.push(signal);
    }
}
