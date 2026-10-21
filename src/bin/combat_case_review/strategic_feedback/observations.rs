use sts_oracle_runtime::ai::strategy::deck_strategic_deficit::DeckStrategicDeficit;
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::super::classification::CombatGapReviewClassification;
use super::super::search_types::SearchDiagnosticProgressFacts;
use super::types::CombatStrategicFeedbackObservations;

pub(super) fn feedback_observations(
    case: &CombatCase,
    static_deficit: &DeckStrategicDeficit,
    classification: &CombatGapReviewClassification,
    progress: Option<&SearchDiagnosticProgressFacts>,
) -> CombatStrategicFeedbackObservations {
    CombatStrategicFeedbackObservations {
        review_kind: classification.kind,
        focus_source: progress.map(|progress| progress.source),
        focus_terminal: progress.map(|progress| progress.terminal),
        focus_estimated: progress.map(|progress| progress.estimated),
        focus_final_hp: progress.map(|progress| progress.final_hp),
        focus_hp_loss: progress.map(|progress| progress.hp_loss),
        focus_living_enemy_count: progress.map(|progress| progress.living_enemy_count),
        focus_total_enemy_hp: progress.map(|progress| progress.total_enemy_hp),
        enemy_count: case.core.combat.enemies.len(),
        hp_ratio_pct: if case.core.run.max_hp > 0 {
            case.core.run.hp * 100 / case.core.run.max_hp
        } else {
            0
        },
        static_frontload: static_deficit.frontload_damage,
        static_aoe: static_deficit.aoe_or_minion_control,
        static_block: static_deficit.block_or_mitigation,
        static_scaling: static_deficit.boss_scaling_plan,
        static_burden: static_deficit.deck_burden,
    }
}
