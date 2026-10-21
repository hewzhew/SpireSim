use std::time::Duration;

use crate::sim::combat::{CombatPosition, EngineCombatStepper};

use super::super::{
    run_combat_search_v2, CombatSearchV2ActionTrace, CombatSearchV2Config,
    CombatSearchV2TrajectoryReport, SearchTerminalLabel,
};
use super::replay::{replay_actions, replay_prefix};
use super::types::{CombatLineLabCutReport, CutPoint};

pub(super) fn repair_cut(
    start: &CombatPosition,
    parent: &CombatSearchV2TrajectoryReport,
    cut: CutPoint,
    config: &CombatSearchV2Config,
    per_cut_ms: u64,
) -> CombatLineLabCutReport {
    let stepper = EngineCombatStepper;
    let Some(prefix) = replay_prefix(start, &parent.actions[..cut.action_index], &stepper) else {
        return failed_cut(cut, "prefix_replay_failed");
    };

    let original_suffix = &parent.actions[cut.action_index..];
    let baseline_suffix = replay_actions(&prefix.position, original_suffix, &stepper);
    let baseline_suffix_replay_ok = baseline_suffix.as_ref().map(|summary| {
        summary.terminal == parent.terminal
            && summary.final_hp == parent.final_hp
            && summary.total_enemy_hp == parent.final_state.total_enemy_hp
            && summary.living_enemy_count == parent.final_state.living_enemy_count
    });

    let mut suffix_config = config.clone();
    suffix_config.wall_time = Some(Duration::from_millis(per_cut_ms));
    let suffix = run_combat_search_v2(
        &prefix.position.engine,
        &prefix.position.combat,
        suffix_config,
    );
    let Some(trajectory) = suffix.best_complete_trajectory.as_ref() else {
        return CombatLineLabCutReport {
            cut_kind: cut.kind,
            cut_action_index: cut.action_index,
            prefix_replayed_actions: prefix.replayed_actions,
            terminal: None,
            final_hp: None,
            total_enemy_hp: None,
            living_enemy_count: None,
            turns: None,
            suffix_actions: None,
            total_potions_used: None,
            baseline_suffix_replay_ok,
            baseline_suffix_terminal: baseline_suffix.as_ref().map(|summary| summary.terminal),
            baseline_suffix_final_hp: baseline_suffix.as_ref().map(|summary| summary.final_hp),
            baseline_suffix_total_enemy_hp: baseline_suffix
                .as_ref()
                .map(|summary| summary.total_enemy_hp),
            repair_action_edit_distance: None,
            delta_enemy_hp: None,
            delta_player_hp: None,
            nodes_expanded: Some(suffix.stats.nodes_expanded),
            deadline_hit: Some(suffix.stats.deadline_hit),
            failed_reason: Some("no_suffix_complete"),
        };
    };

    CombatLineLabCutReport {
        cut_kind: cut.kind,
        cut_action_index: cut.action_index,
        prefix_replayed_actions: prefix.replayed_actions,
        terminal: Some(trajectory.terminal),
        final_hp: Some(trajectory.final_hp),
        total_enemy_hp: Some(trajectory.final_state.total_enemy_hp),
        living_enemy_count: Some(trajectory.final_state.living_enemy_count),
        turns: Some(trajectory.turns),
        suffix_actions: Some(trajectory.actions.len()),
        total_potions_used: Some(prefix.potions_used + trajectory.potions_used),
        baseline_suffix_replay_ok,
        baseline_suffix_terminal: baseline_suffix.as_ref().map(|summary| summary.terminal),
        baseline_suffix_final_hp: baseline_suffix.as_ref().map(|summary| summary.final_hp),
        baseline_suffix_total_enemy_hp: baseline_suffix
            .as_ref()
            .map(|summary| summary.total_enemy_hp),
        repair_action_edit_distance: Some(action_edit_distance(
            original_suffix,
            &trajectory.actions,
        )),
        delta_enemy_hp: Some(
            trajectory.final_state.total_enemy_hp - parent.final_state.total_enemy_hp,
        ),
        delta_player_hp: Some(trajectory.final_hp - parent.final_hp),
        nodes_expanded: Some(suffix.stats.nodes_expanded),
        deadline_hit: Some(suffix.stats.deadline_hit),
        failed_reason: None,
    }
}

pub(super) fn repair_rank(cut: &CombatLineLabCutReport) -> (i32, i32, i32) {
    let terminal_rank = match cut.terminal {
        Some(SearchTerminalLabel::Win) => 2,
        Some(SearchTerminalLabel::Unresolved) => 1,
        _ => 0,
    };
    (
        terminal_rank,
        -cut.total_enemy_hp.unwrap_or(i32::MAX),
        cut.final_hp.unwrap_or(i32::MIN),
    )
}

fn failed_cut(cut: CutPoint, reason: &'static str) -> CombatLineLabCutReport {
    CombatLineLabCutReport {
        cut_kind: cut.kind,
        cut_action_index: cut.action_index,
        prefix_replayed_actions: 0,
        terminal: None,
        final_hp: None,
        total_enemy_hp: None,
        living_enemy_count: None,
        turns: None,
        suffix_actions: None,
        total_potions_used: None,
        baseline_suffix_replay_ok: None,
        baseline_suffix_terminal: None,
        baseline_suffix_final_hp: None,
        baseline_suffix_total_enemy_hp: None,
        repair_action_edit_distance: None,
        delta_enemy_hp: None,
        delta_player_hp: None,
        nodes_expanded: None,
        deadline_hit: None,
        failed_reason: Some(reason),
    }
}

fn action_edit_distance(
    left: &[CombatSearchV2ActionTrace],
    right: &[CombatSearchV2ActionTrace],
) -> usize {
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (i, left_action) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_action) in right.iter().enumerate() {
            let substitution = usize::from(left_action.action_key != right_action.action_key);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}
