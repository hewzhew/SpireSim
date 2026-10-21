use sts_oracle_runtime::ai::combat_search_v2::{
    run_combat_turn_pool_opening_report_v0, CombatTurnPoolOpeningReport,
};
use sts_oracle_runtime::content::monsters::EnemyId;
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::options::ReviewOptions;
use super::search_runner::review_all_potions_profile;

pub(super) fn run_awakened_opening_probe(
    options: &ReviewOptions,
    case: &CombatCase,
) -> Option<CombatTurnPoolOpeningReport> {
    if !options.awakened_opening_probe || !has_awakened_one(case) {
        return None;
    }
    let config = review_all_potions_profile(
        "awakened_opening_probe",
        options.slow_nodes,
        options.awakened_opening_probe_ms,
        options,
    )
    .to_config();
    Some(run_combat_turn_pool_opening_report_v0(
        &case.core.position,
        options.awakened_opening_probe_ms,
        options.awakened_opening_probe_turns,
        Some(&config),
    ))
}

fn has_awakened_one(case: &CombatCase) -> bool {
    case.core
        .position
        .combat
        .entities
        .monsters
        .iter()
        .any(|monster| monster.enemy_id == EnemyId::AwakenedOne)
}
