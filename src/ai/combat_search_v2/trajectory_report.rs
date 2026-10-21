use super::frontier::RootLineage;
use super::*;

#[allow(clippy::too_many_arguments)]
pub fn trajectory_from_state(
    engine: EngineState,
    combat: CombatState,
    initial_hp: i32,
    actions: Vec<CombatSearchV2ActionTrace>,
    potions_used: u32,
    potions_discarded: u32,
    cards_played: u32,
    estimated: bool,
) -> CombatSearchV2TrajectoryReport {
    let node = SearchNode {
        engine,
        combat,
        actions,
        turn_prefix: TurnPrefixState::default(),
        initial_hp,
        potions_used,
        potions_discarded,
        cards_played,
        potion_tactical_priority: 0,
        last_turn_branch_priority: 0,
        action_prior_score: None,
        action_ordering_frontier_hint: 0,
        rollout_estimate: RolloutNodeEstimate::unevaluated(),
        root_lineage: RootLineage::Unmaterialized,
    };
    trajectory_report(&node, estimated)
}

pub(super) fn trajectory_report(
    node: &SearchNode,
    estimated: bool,
) -> CombatSearchV2TrajectoryReport {
    CombatSearchV2TrajectoryReport {
        terminal: terminal_label(&node.engine, &node.combat),
        estimated,
        outcome_order_key: CombatOutcomeScore::from_node(node).to_report_key(),
        actions: node.actions.clone(),
        final_hp: node.combat.entities.player.current_hp,
        final_max_hp: node.combat.entities.player.max_hp,
        persistent_run_value: super::external_payoff::materialized_persistent_payoff_score_v1(
            &node.combat,
        ),
        final_block: node.combat.entities.player.block,
        hp_loss: (node.initial_hp - node.combat.entities.player.current_hp).max(0),
        turns: node.combat.turn.turn_count,
        potions_used: node.potions_used,
        potions_discarded: node.potions_discarded,
        cards_played: node.cards_played,
        enemy_final_state: node
            .combat
            .entities
            .monsters
            .iter()
            .enumerate()
            .map(|(slot, monster)| summarize_enemy(&node.combat, slot, monster))
            .collect(),
        final_state: summarize_state(&node.engine, &node.combat),
    }
}

pub(super) fn summarize_state(
    engine: &EngineState,
    combat: &CombatState,
) -> CombatSearchV2StateSummary {
    CombatSearchV2StateSummary {
        engine_state: engine.kind_v1().to_string(),
        terminal: terminal_label(engine, combat),
        player_hp: combat.entities.player.current_hp,
        player_block: combat.entities.player.block,
        energy: combat.turn.energy,
        turn_count: combat.turn.turn_count,
        living_enemy_count: living_enemy_count(combat),
        total_enemy_hp: total_living_enemy_hp(combat),
        visible_incoming_damage: visible_incoming_damage(combat),
        enemy_slots: combat
            .entities
            .monsters
            .iter()
            .enumerate()
            .map(|(slot, monster)| summarize_enemy(combat, slot, monster))
            .collect(),
        hand_count: combat.zones.hand.len(),
        draw_count: combat.zones.draw_pile.len(),
        discard_count: combat.zones.discard_pile.len(),
        exhaust_count: combat.zones.exhaust_pile.len(),
        limbo_count: combat.zones.limbo.len(),
        queued_cards_count: combat.zones.queued_cards.len(),
    }
}

fn summarize_enemy(
    combat: &CombatState,
    slot: usize,
    monster: &MonsterEntity,
) -> CombatSearchV2EnemySummary {
    CombatSearchV2EnemySummary {
        slot,
        entity_id: monster.id,
        enemy_id: monster.enemy_id.stable_id().to_owned(),
        hp: monster.current_hp,
        max_hp: monster.max_hp,
        block: monster.block,
        alive: monster.is_alive_for_action(),
        escaped: monster.is_escaped,
        dying: monster.is_dying,
        half_dead: monster.half_dead,
        phase: enemy_phase_label(monster),
        planned_move_id: monster.planned_move_id(),
        visible_intent: format!("{:?}", combat.monster_protocol_visible_intent(monster.id)),
        visible_incoming_damage: if monster.is_alive_for_action() {
            monster_preview_total_damage_in_combat(combat, monster)
        } else {
            0
        },
    }
}

fn enemy_phase_label(monster: &MonsterEntity) -> Option<&'static str> {
    match monster.enemy_id {
        EnemyId::AwakenedOne if monster.awakened_one.form1 => Some("form_one"),
        EnemyId::AwakenedOne if monster.half_dead => Some("rebirth_pending"),
        EnemyId::AwakenedOne => Some("form_two"),
        _ => None,
    }
}

#[cfg(test)]
mod phase_summary_tests {
    use super::*;

    #[test]
    fn awakened_one_summary_distinguishes_both_forms_and_rebirth_window() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut awakened = crate::test_support::test_monster(EnemyId::AwakenedOne);
        awakened.awakened_one.form1 = true;
        combat.entities.monsters = vec![awakened];

        assert_eq!(
            summarize_state(&EngineState::CombatPlayerTurn, &combat).enemy_slots[0].phase,
            Some("form_one")
        );

        combat.entities.monsters[0].awakened_one.form1 = false;
        combat.entities.monsters[0].half_dead = true;
        assert_eq!(
            summarize_state(&EngineState::CombatPlayerTurn, &combat).enemy_slots[0].phase,
            Some("rebirth_pending")
        );

        combat.entities.monsters[0].half_dead = false;
        assert_eq!(
            summarize_state(&EngineState::CombatPlayerTurn, &combat).enemy_slots[0].phase,
            Some("form_two")
        );
    }
}
