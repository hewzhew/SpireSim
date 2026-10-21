use sts_oracle_runtime::ai::combat_search_v2::replay_combat_search_witness_line_v0;
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::super::options::ReviewOptions;
use super::super::quality_lanes::{
    combat_line_quality, compare_quality, quality_lane_specs, witness_line_from_trajectory,
};
use super::super::search_runner::run_config_search;
use super::super::search_types::SearchDiagnosticProgressFacts;
use super::targets::combat_case_with_player_hp;
use super::types::{
    CounterfactualHpCandidate, CounterfactualHpLevel, CounterfactualHpProgress,
    CounterfactualHpReplay,
};

pub(super) fn run_counterfactual_hp_level(
    options: &ReviewOptions,
    original_case: &CombatCase,
    label: String,
    hp: i32,
) -> CounterfactualHpLevel {
    let case = combat_case_with_player_hp(original_case, hp);
    let specs = quality_lane_specs();
    let lane_count = specs.len().max(1);
    let total_nodes = options
        .quality_lane_total_nodes
        .unwrap_or(options.slow_nodes)
        .max(1);
    let total_wall_ms = options
        .quality_lane_total_ms
        .unwrap_or(options.slow_ms)
        .max(1);
    let per_lane_nodes = (total_nodes / lane_count).max(1);
    let per_lane_wall_ms = (total_wall_ms / lane_count as u64).max(1);
    let mut best: Option<CounterfactualHpCandidate> = None;
    let mut best_progress: Option<CounterfactualHpProgress> = None;
    let mut total_terminal_wins = 0;
    for spec in specs {
        let (review, report) = run_config_search(
            spec.label,
            &case,
            spec.config(per_lane_nodes, per_lane_wall_ms),
            options.action_preview_limit,
        );
        total_terminal_wins += review.terminal_wins;
        if let Some(facts) = review.facts.diagnostic_progress.clone() {
            let progress = CounterfactualHpProgress {
                lane: spec.label,
                facts,
            };
            if best_progress
                .as_ref()
                .is_none_or(|current| progress_facts_better(&progress.facts, &current.facts))
            {
                best_progress = Some(progress);
            }
        }
        let quality = combat_line_quality(&report);
        let witness = report
            .best_win_trajectory
            .as_ref()
            .map(|trajectory| witness_line_from_trajectory(spec.label, trajectory));
        if let (Some(quality), Some(witness)) = (quality, witness) {
            let candidate = CounterfactualHpCandidate {
                lane: spec.label,
                review,
                quality,
                witness,
            };
            if best
                .as_ref()
                .is_none_or(|current| compare_quality(&candidate.quality, &current.quality).is_gt())
            {
                best = Some(candidate);
            }
        }
    }
    let replay_on_original_hp = best.as_ref().map(|candidate| {
        let replay =
            replay_combat_search_witness_line_v0(&original_case.core.position, &candidate.witness);
        CounterfactualHpReplay {
            terminal: replay.terminal,
            final_hp: replay.final_hp,
            total_enemy_hp: replay.total_enemy_hp,
            living_enemy_count: replay.living_enemy_count,
            replayed_actions: replay.replayed_actions,
            action_count: replay.action_count,
        }
    });
    CounterfactualHpLevel {
        label,
        hp,
        selected_lane: best.as_ref().map(|candidate| candidate.lane),
        complete_win: best.is_some(),
        quality: best.as_ref().map(|candidate| candidate.quality.clone()),
        best_progress,
        nodes_to_first_win: best
            .as_ref()
            .and_then(|candidate| candidate.review.nodes_to_first_win),
        total_terminal_wins,
        replay_on_original_hp,
    }
}

fn progress_facts_better(
    left: &SearchDiagnosticProgressFacts,
    right: &SearchDiagnosticProgressFacts,
) -> bool {
    (
        left.half_dead_enemy_count as i64,
        -(left.living_enemy_count as i64),
        -(left.total_enemy_hp as i64),
        left.final_hp as i64,
        left.turns as i64,
        -(left.potions_used as i64),
        -(left.action_count.unwrap_or(usize::MAX) as i64),
    ) > (
        right.half_dead_enemy_count as i64,
        -(right.living_enemy_count as i64),
        -(right.total_enemy_hp as i64),
        right.final_hp as i64,
        right.turns as i64,
        -(right.potions_used as i64),
        -(right.action_count.unwrap_or(usize::MAX) as i64),
    )
}
