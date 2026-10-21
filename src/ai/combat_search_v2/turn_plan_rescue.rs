use std::time::Duration;

use crate::sim::combat::CombatPosition;
use crate::state::core::ClientInput;

use super::{
    enumerate_combat_search_v2_turn_plan_probe_candidates, run_combat_search_v2,
    CombatSearchV2ActionTrace, CombatSearchV2Config, CombatSearchV2Satisfaction,
    CombatSearchV2TurnPlanPolicy, SearchTerminalLabel,
};

const MAX_RESCUE_PLANS: usize = 2;

#[derive(Clone, Debug)]
pub struct CombatTurnPlanRescueWin {
    pub plan_index: usize,
    pub prefix_action_count: usize,
    pub actions: Vec<CombatSearchV2ActionTrace>,
    pub plans_attempted: usize,
    pub nodes_expanded: u64,
    pub nodes_generated: u64,
    pub deadline_hit: bool,
}

impl CombatTurnPlanRescueWin {
    pub fn transition_summary(&self) -> String {
        format!(
            "turn_plan_rescue plan={} prefix_actions={} total_actions={} plans_attempted={} nodes={}/{} deadline_hit={}",
            self.plan_index,
            self.prefix_action_count,
            self.actions.len(),
            self.plans_attempted,
            self.nodes_expanded,
            self.nodes_generated,
            self.deadline_hit
        )
    }
}

pub fn find_combat_turn_plan_rescue_win_v0(
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    per_plan_budget_ms: u64,
    max_hp_loss: Option<u32>,
) -> Option<CombatTurnPlanRescueWin> {
    if !matches!(
        start.engine,
        crate::state::core::EngineState::CombatPlayerTurn
    ) {
        return None;
    }
    let enumeration =
        enumerate_combat_search_v2_turn_plan_probe_candidates(&start.engine, &start.combat, config);
    let initial_hp = start.combat.entities.player.current_hp;
    let minimum_final_hp = max_hp_loss
        .map(|limit| initial_hp.saturating_sub(limit as i32))
        .unwrap_or(1)
        .max(1);
    let mut plans_attempted = 0usize;
    let mut nodes_expanded = 0u64;
    let mut nodes_generated = 0u64;
    let mut deadline_hit = false;

    for candidate in enumeration.candidates.into_iter().take(MAX_RESCUE_PLANS) {
        if candidate.position.combat.entities.player.current_hp < minimum_final_hp {
            continue;
        }
        plans_attempted = plans_attempted.saturating_add(1);
        let prefix_action_count = candidate.report.actions.len();

        if candidate.report.end_state.terminal == SearchTerminalLabel::Win {
            return Some(CombatTurnPlanRescueWin {
                plan_index: candidate.report.plan_index,
                prefix_action_count,
                actions: reindex_actions(candidate.report.actions),
                plans_attempted,
                nodes_expanded,
                nodes_generated,
                deadline_hit,
            });
        }
        if candidate.report.end_state.terminal == SearchTerminalLabel::Loss {
            continue;
        }

        let child_config = rescue_child_config(
            config,
            &candidate.report.actions,
            candidate.position.combat.entities.player.current_hp,
            per_plan_budget_ms,
            minimum_final_hp,
        );
        if child_config.max_actions_per_line == 0 {
            continue;
        }
        let report = run_combat_search_v2(
            &candidate.position.engine,
            &candidate.position.combat,
            child_config,
        );
        nodes_expanded = nodes_expanded.saturating_add(report.stats.nodes_expanded);
        nodes_generated = nodes_generated.saturating_add(report.stats.nodes_generated);
        deadline_hit |= report.stats.deadline_hit;

        let Some(trajectory) = report.best_win_trajectory else {
            continue;
        };
        if trajectory.final_hp < minimum_final_hp {
            continue;
        }
        let mut actions = candidate.report.actions;
        actions.extend(trajectory.actions);
        return Some(CombatTurnPlanRescueWin {
            plan_index: candidate.report.plan_index,
            prefix_action_count,
            actions: reindex_actions(actions),
            plans_attempted,
            nodes_expanded,
            nodes_generated,
            deadline_hit,
        });
    }
    None
}

fn rescue_child_config(
    config: &CombatSearchV2Config,
    prefix: &[CombatSearchV2ActionTrace],
    child_start_hp: i32,
    per_plan_budget_ms: u64,
    minimum_final_hp: i32,
) -> CombatSearchV2Config {
    let mut child = config.clone();
    child.max_actions_per_line = child.max_actions_per_line.saturating_sub(prefix.len());
    child.wall_time = Some(Duration::from_millis(per_plan_budget_ms.max(1)));
    child.satisfaction = CombatSearchV2Satisfaction::HpLossAtMost(
        child_start_hp.saturating_sub(minimum_final_hp).max(0) as u32,
    );
    child.turn_plan_policy = CombatSearchV2TurnPlanPolicy::Disabled;
    child.max_potions_used = child.max_potions_used.map(|limit| {
        limit.saturating_sub(
            prefix
                .iter()
                .filter(|action| matches!(action.input, ClientInput::UsePotion { .. }))
                .count() as u32,
        )
    });
    child.input_label = Some(format!(
        "{}:turn-plan-rescue-child",
        config.input_label.as_deref().unwrap_or("combat")
    ));
    child
}

fn reindex_actions(mut actions: Vec<CombatSearchV2ActionTrace>) -> Vec<CombatSearchV2ActionTrace> {
    for (step_index, action) in actions.iter_mut().enumerate() {
        action.step_index = step_index;
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_budget_preserves_whole_line_action_and_potion_limits() {
        let mut config = CombatSearchV2Config {
            max_actions_per_line: 9,
            max_potions_used: Some(1),
            turn_plan_policy: CombatSearchV2TurnPlanPolicy::RootFrontierSeed,
            ..CombatSearchV2Config::default()
        };
        config.input_label = Some("root".to_string());
        let prefix = vec![
            trace(7, ClientInput::DiscardPotion(0)),
            trace(
                8,
                ClientInput::UsePotion {
                    potion_index: 0,
                    target: None,
                },
            ),
            trace(9, ClientInput::EndTurn),
        ];

        let child = rescue_child_config(&config, &prefix, 17, 321, 5);

        assert_eq!(child.max_actions_per_line, 6);
        assert_eq!(child.max_potions_used, Some(0));
        assert_eq!(child.wall_time, Some(Duration::from_millis(321)));
        assert_eq!(
            child.satisfaction,
            CombatSearchV2Satisfaction::HpLossAtMost(12)
        );
        assert_eq!(
            child.turn_plan_policy,
            CombatSearchV2TurnPlanPolicy::Disabled
        );
    }

    #[test]
    fn combined_rescue_line_has_contiguous_step_indexes() {
        let actions = reindex_actions(vec![
            trace(41, ClientInput::EndTurn),
            trace(3, ClientInput::EndTurn),
        ]);

        assert_eq!(
            actions
                .iter()
                .map(|action| action.step_index)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(actions[0].action_id, 41);
        assert_eq!(actions[1].action_id, 3);
    }

    #[test]
    fn rescue_declines_pending_choice_owned_by_action_prefix_search() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.draw_pile = ((0..13)
            .map(|index| {
                crate::runtime::combat::CombatCard::for_scenario(
                    crate::content::cards::CardId::Strike,
                    1_000 + index,
                )
            })
            .collect::<Vec<_>>())
        .into();
        let start = CombatPosition::new(
            crate::state::core::EngineState::PendingChoice(
                crate::state::core::PendingChoice::ScrySelect {
                    cards: vec![crate::content::cards::CardId::Strike; 13],
                    card_lineages: crate::test_support::scenario_lineages(1_000..1_013),
                },
            ),
            combat,
        );

        assert!(find_combat_turn_plan_rescue_win_v0(
            &start,
            &CombatSearchV2Config::default(),
            10,
            None,
        )
        .is_none());
    }

    fn trace(action_id: usize, input: ClientInput) -> CombatSearchV2ActionTrace {
        CombatSearchV2ActionTrace {
            step_index: action_id,
            action_id,
            action_key: format!("action-{action_id}"),
            action_debug: format!("action {action_id}"),
            input,
        }
    }
}
