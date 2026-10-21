use std::cmp::Reverse;
use std::collections::HashSet;

use crate::content::monsters::EnemyId;
use crate::sim::combat::CombatPosition;

use super::super::{trajectory_report::summarize_state, SearchTerminalLabel};
use super::types::{
    CombatTurnPoolOpeningLineReport, CombatTurnPoolRescueLineSummary, TurnPoolLane, TurnPoolNode,
};

pub(super) fn keep_lane_nodes(nodes: &mut Vec<TurnPoolNode>, lane: TurnPoolLane, limit: usize) {
    nodes.sort_by_key(|node| Reverse(lane_rank(node, lane)));
    nodes.truncate(limit);
}

pub(super) fn keep_diverse_lane_nodes(
    nodes: &mut Vec<TurnPoolNode>,
    lane: TurnPoolLane,
    limit: usize,
) {
    if nodes.len() <= limit {
        nodes.sort_by_key(|node| Reverse(lane_rank(node, lane)));
        return;
    }
    if limit == 0 {
        nodes.clear();
        return;
    }

    // A single lexicographic beam drops action-order setup such as block/draw
    // before damage. Keep both extremes plus candidates that rank reasonably
    // under both views, without adding unlike units into one magic score.
    let candidates = std::mem::take(nodes);
    let mut primary = (0..candidates.len()).collect::<Vec<_>>();
    primary.sort_by_key(|&index| Reverse(lane_rank(&candidates[index], lane)));
    let mut continuation = (0..candidates.len()).collect::<Vec<_>>();
    continuation.sort_by_key(|&index| Reverse(turn_continuation_rank(&candidates[index])));

    let mut primary_ranks = vec![0usize; candidates.len()];
    let mut continuation_ranks = vec![0usize; candidates.len()];
    for (rank, &index) in primary.iter().enumerate() {
        primary_ranks[index] = rank;
    }
    for (rank, &index) in continuation.iter().enumerate() {
        continuation_ranks[index] = rank;
    }
    let mut compromise = (0..candidates.len()).collect::<Vec<_>>();
    compromise.sort_by_key(|&index| {
        let primary_rank = primary_ranks[index];
        let continuation_rank = continuation_ranks[index];
        (
            primary_rank.max(continuation_rank),
            primary_rank.saturating_add(continuation_rank),
            primary_rank,
        )
    });

    let per_extreme_quota = (limit / 4).max(1);
    // Remaining energy is a mechanical continuation resource. Preserve one
    // compromise candidate per bucket so zero-cost access cannot be crowded
    // out by either immediate damage or maximum-block sequences.
    let mut energy_buckets = HashSet::new();
    let energy_diverse = compromise
        .iter()
        .copied()
        .filter(|&index| energy_buckets.insert(candidates[index].position.combat.turn.energy))
        .collect::<Vec<_>>();
    let mut selected = Vec::with_capacity(limit);
    let mut seen = HashSet::with_capacity(limit);
    append_unique_nodes(
        &mut selected,
        &mut seen,
        &candidates,
        primary.iter().take(per_extreme_quota).copied(),
        limit,
    );
    append_unique_nodes(
        &mut selected,
        &mut seen,
        &candidates,
        continuation.iter().take(per_extreme_quota).copied(),
        limit,
    );
    append_unique_nodes(&mut selected, &mut seen, &candidates, energy_diverse, limit);
    append_unique_nodes(&mut selected, &mut seen, &candidates, compromise, limit);
    append_unique_nodes(
        &mut selected,
        &mut seen,
        &candidates,
        primary.into_iter().chain(continuation),
        limit,
    );
    *nodes = selected;
}

fn append_unique_nodes(
    selected: &mut Vec<TurnPoolNode>,
    seen: &mut HashSet<usize>,
    nodes: &[TurnPoolNode],
    candidates: impl IntoIterator<Item = usize>,
    limit: usize,
) {
    for index in candidates {
        if selected.len() >= limit {
            break;
        }
        if seen.insert(index) {
            selected.push(nodes[index].clone());
        }
    }
}

fn turn_continuation_rank(node: &TurnPoolNode) -> (i32, i32, i32, i32, i32, i32) {
    let combat = &node.position.combat;
    (
        terminal_rank_for_line(node.terminal),
        combat
            .entities
            .player
            .current_hp
            .saturating_sub(visible_pressure(&node.position)),
        combat.turn.energy as i32,
        combat.zones.hand.len() as i32,
        -total_enemy_hp(&node.position),
        -(node.actions.len() as i32),
    )
}

pub(super) fn lane_rank(node: &TurnPoolNode, lane: TurnPoolLane) -> (i32, i32, i32, i32, i32, i32) {
    let terminal = terminal_rank_for_line(node.terminal);
    let hp = node.position.combat.entities.player.current_hp;
    let enemy_hp = total_enemy_hp(&node.position);
    match lane {
        TurnPoolLane::Damage => (
            terminal,
            -enemy_hp,
            hp,
            -(node.actions.len() as i32),
            -(node.potions_used as i32),
            0,
        ),
        TurnPoolLane::Survival => (
            terminal,
            hp,
            -visible_pressure(&node.position),
            -enemy_hp,
            -(node.potions_used as i32),
            0,
        ),
        TurnPoolLane::Setup => (
            terminal,
            node.powers_played as i32,
            hp,
            -enemy_hp,
            -(node.actions.len() as i32),
            0,
        ),
        TurnPoolLane::PowerDelay => (
            terminal,
            -(node.powers_played as i32),
            -enemy_hp,
            hp,
            -(node.potions_used as i32),
            0,
        ),
        TurnPoolLane::PotionBurst => (
            terminal,
            node.potions_used as i32,
            -enemy_hp,
            hp,
            -(node.actions.len() as i32),
            0,
        ),
        TurnPoolLane::CultistCleanup => {
            let (cultists_alive, total_cultist_hp) = cultist_pressure(&node.position);
            (
                terminal,
                -(cultists_alive as i32),
                -total_cultist_hp,
                hp,
                -enemy_hp,
                -(node.actions.len() as i32),
            )
        }
    }
}

pub(super) fn turn_pool_opening_line_report(
    lane: TurnPoolLane,
    node: &TurnPoolNode,
) -> CombatTurnPoolOpeningLineReport {
    let (cultists_alive, total_cultist_hp) = cultist_pressure(&node.position);
    CombatTurnPoolOpeningLineReport {
        lane: lane.label(),
        terminal: node.terminal,
        final_hp: node.position.combat.entities.player.current_hp,
        turns: node.position.combat.turn.turn_count,
        actions: node.actions.clone(),
        potions_used: node.potions_used,
        powers_played: node.powers_played,
        cultists_alive,
        total_cultist_hp,
        end_state: summarize_state(&node.position.engine, &node.position.combat),
    }
}

pub(super) fn opening_cleanup_rank(
    line: &CombatTurnPoolOpeningLineReport,
) -> (i32, i32, i32, i32, i32) {
    (
        terminal_rank_for_line(line.terminal),
        -(line.cultists_alive as i32),
        -line.total_cultist_hp,
        line.final_hp,
        -(line.actions.len() as i32),
    )
}

pub(super) fn turn_pool_summary(
    lane: TurnPoolLane,
    node: &TurnPoolNode,
) -> CombatTurnPoolRescueLineSummary {
    CombatTurnPoolRescueLineSummary {
        lane: lane.label(),
        terminal: node.terminal,
        final_hp: node.position.combat.entities.player.current_hp,
        total_enemy_hp: total_enemy_hp(&node.position),
        living_enemy_count: living_enemy_count(&node.position),
        turns: node.position.combat.turn.turn_count,
        actions: node.actions.len(),
        potions_used: node.potions_used,
        powers_played: node.powers_played,
    }
}

pub(super) fn turn_pool_summary_rank(
    line: &CombatTurnPoolRescueLineSummary,
) -> (i32, i32, i32, i32) {
    let terminal_non_win = matches!(
        line.terminal,
        SearchTerminalLabel::Escape | SearchTerminalLabel::Loss
    );
    (
        turn_pool_summary_tier(line),
        if terminal_non_win {
            -line.total_enemy_hp
        } else {
            line.turns as i32
        },
        if terminal_non_win {
            line.turns as i32
        } else {
            -line.total_enemy_hp
        },
        line.final_hp,
    )
}

fn turn_pool_summary_tier(line: &CombatTurnPoolRescueLineSummary) -> i32 {
    match line.terminal {
        SearchTerminalLabel::Win => 4,
        SearchTerminalLabel::Loss if line.living_enemy_count == 1 && line.total_enemy_hp <= 50 => 3,
        SearchTerminalLabel::Unresolved => 2,
        SearchTerminalLabel::Escape | SearchTerminalLabel::Loss => 1,
    }
}

fn terminal_rank_for_line(terminal: SearchTerminalLabel) -> i32 {
    match terminal {
        SearchTerminalLabel::Win => 2,
        SearchTerminalLabel::Unresolved => 1,
        SearchTerminalLabel::Escape | SearchTerminalLabel::Loss => 0,
    }
}

fn living_enemy_count(position: &CombatPosition) -> usize {
    position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .count()
}

fn total_enemy_hp(position: &CombatPosition) -> i32 {
    position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.current_hp.max(0) + monster.block.max(0))
        .sum()
}

fn cultist_pressure(position: &CombatPosition) -> (usize, i32) {
    let cultists =
        position.combat.entities.monsters.iter().filter(|monster| {
            monster.is_alive_for_action() && monster.enemy_id == EnemyId::Cultist
        });
    cultists.fold((0usize, 0i32), |(count, hp), monster| {
        (
            count.saturating_add(1),
            hp.saturating_add(monster.current_hp.max(0)),
        )
    })
}

fn visible_pressure(position: &CombatPosition) -> i32 {
    position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| {
            crate::sim::combat_projection::monster_preview_total_damage_in_combat(
                &position.combat,
                monster,
            )
        })
        .sum::<i32>()
        .saturating_sub(position.combat.entities.player.block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::sim::combat::{CombatPosition, EngineCombatStepper};
    use crate::state::core::EngineState;
    use crate::test_support::{blank_test_combat, test_monster};

    #[test]
    fn lane_beam_keeps_defensive_continuation_beside_immediate_damage() {
        let mut nodes = (20..33)
            .map(|cultist_hp| test_node(cultist_hp, 0))
            .collect::<Vec<_>>();
        nodes.push(test_node(34, 11));

        keep_diverse_lane_nodes(&mut nodes, TurnPoolLane::Damage, 12);

        assert_eq!(nodes.len(), 12);
        assert!(nodes
            .iter()
            .any(|node| { node.position.combat.entities.monsters[0].current_hp == 20 }));
        assert!(nodes
            .iter()
            .any(|node| node.position.combat.entities.player.block == 11));
    }

    fn test_node(cultist_hp: i32, player_block: i32) -> TurnPoolNode {
        let mut combat = blank_test_combat();
        combat.entities.player.current_hp = 41;
        combat.entities.player.block = player_block;
        let mut cultist = test_monster(EnemyId::Cultist);
        cultist.current_hp = cultist_hp;
        cultist.max_hp = 52;
        combat.entities.monsters = vec![cultist];
        TurnPoolNode::root(
            CombatPosition::new(EngineState::CombatPlayerTurn, combat),
            &EngineCombatStepper,
        )
    }
}
