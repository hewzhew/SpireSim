use sts_oracle_runtime::ai::combat_search_v2::CombatLineLabReport;
use sts_oracle_runtime::content::monsters::EnemyId;
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::super::search_types::SearchReview;
use super::collector_assessment::{collector_phase, collector_potion_permission, collector_tags};
use super::collector_objectives::collector_objectives;
use super::collector_start::{collector_start_signals, find_enemy};
use super::line_reviews::collect_line_reviews;
use super::line_tags::aggregate_line_tags;
use super::types::BossPressureLensReport;

pub(super) fn collector_pressure_lens(
    case: &CombatCase,
    ladder: &[SearchReview],
    line_lab: Option<&CombatLineLabReport>,
) -> Option<BossPressureLensReport> {
    let combat = &case.core.position.combat;
    let collector = find_enemy(combat, EnemyId::TheCollector)?;
    let start = collector_start_signals(combat, collector);
    let phase = collector_phase(start.turn);
    let objectives = collector_objectives(&start, phase);
    let potion_permission = collector_potion_permission(&start, phase);
    let line_reviews = collect_line_reviews(ladder, line_lab);
    let mut tags = collector_tags(&start, phase);
    if potion_permission.level == "allow" {
        tags.push("collector_potion_window_open");
    }
    tags.extend(aggregate_line_tags(&line_reviews));
    tags.sort_unstable();
    tags.dedup();

    Some(BossPressureLensReport {
        schema: "boss_pressure_lens_v0",
        boss: "collector",
        phase,
        start,
        tags,
        objectives,
        potion_permission,
        line_reviews,
    })
}
