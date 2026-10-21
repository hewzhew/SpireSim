use crate::content::cards;
use crate::runtime::combat::CombatState;
use crate::sim::combat::{CombatStepper, EngineCombatStepper};
use crate::state::core::EngineState;

use super::frontier::SearchNode;
use super::turn_planner::{
    enumerate_turn_plans, TurnPlanBucket, TurnPlanStopReason, TurnPlanV1, TurnPlannerConfigV1,
};
use super::{
    terminal_label, CombatSearchV2Config, CombatSearchV2TrajectoryReport, SearchTerminalLabel,
};

#[derive(Clone, Debug)]
pub struct CombatSearchV2TurnSegmentReport {
    pub behavior_label: &'static str,
    pub source: &'static str,
    pub candidate_count: usize,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub selected_bucket: Option<&'static str>,
    pub selected_stop_reason: Option<&'static str>,
    pub selected: Option<CombatSearchV2TrajectoryReport>,
}

pub fn plan_combat_turn_segment_v1(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
) -> CombatSearchV2TurnSegmentReport {
    plan_combat_turn_segment_with_stepper_v1(engine, combat, config, &EngineCombatStepper)
}

fn plan_combat_turn_segment_with_stepper_v1(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
    stepper: &impl CombatStepper,
) -> CombatSearchV2TurnSegmentReport {
    let root = SearchNode::root(engine.clone(), combat.clone());
    let turn_config = TurnPlannerConfigV1 {
        max_inner_nodes: 512,
        max_partial_frontier_states: None,
        max_end_states: 24,
        per_bucket_limit: 4,
        potion_policy: config.potion_policy,
        max_potions_used: config.max_potions_used,
        allowed_potion_slots: config.allowed_potion_slots,
        allow_potion_discard: config.allow_potion_discard,
        max_engine_steps_per_action: config.max_engine_steps_per_action,
        turn_plan_prior: config.turn_plan_prior.clone(),
        capture_step_trace: false,
    };
    let enumeration = enumerate_turn_plans(&root, stepper, &turn_config, None);
    let selected = enumeration
        .plans
        .iter()
        .filter(|plan| turn_segment_plan_is_eligible(combat, plan))
        .max_by(|left, right| {
            left.eval
                .cmp_core(&right.eval)
                .then_with(|| {
                    turn_segment_preserved_ethereal_resource_count(combat, left).cmp(
                        &turn_segment_preserved_ethereal_resource_count(combat, right),
                    )
                })
                .then_with(|| left.eval.cmp(&right.eval))
        });

    CombatSearchV2TurnSegmentReport {
        behavior_label: "partial_segment_not_terminal_claim",
        source: "turn_plan_v1_current_turn_exact_enumeration",
        candidate_count: enumeration.plans.len(),
        nodes_expanded: enumeration.nodes_expanded,
        nodes_generated: enumeration.nodes_generated,
        selected_bucket: selected.map(|plan| plan.bucket.label()),
        selected_stop_reason: selected.map(|plan| plan.stop_reason.label()),
        selected: selected.map(|plan| {
            super::trajectory_report::trajectory_report(
                &plan.end_node,
                terminal_label(&plan.end_node.engine, &plan.end_node.combat)
                    == SearchTerminalLabel::Unresolved,
            )
        }),
    }
}

fn turn_segment_plan_is_eligible(root_combat: &CombatState, plan: &&TurnPlanV1) -> bool {
    if plan.actions.is_empty() {
        return false;
    }
    if !matches!(
        plan.stop_reason,
        TurnPlanStopReason::NextTurn | TurnPlanStopReason::Terminal
    ) {
        return false;
    }
    let terminal = terminal_label(&plan.end_node.engine, &plan.end_node.combat);
    if terminal == SearchTerminalLabel::Loss || plan.bucket == TurnPlanBucket::TerminalLoss {
        return false;
    }
    let final_hp = plan.end_node.combat.entities.player.current_hp;
    if final_hp <= 0 {
        return false;
    }
    let hp_loss = (root_combat.entities.player.current_hp - final_hp).max(0);
    hp_loss <= turn_segment_hp_loss_limit(root_combat)
}

fn turn_segment_hp_loss_limit(combat: &CombatState) -> i32 {
    let current_hp = combat.entities.player.current_hp.max(0);
    let max_hp = combat.entities.player.max_hp.max(1);
    let soft_limit = (max_hp / 3).max(24);
    soft_limit.min(current_hp.saturating_sub(1))
}

fn turn_segment_preserved_ethereal_resource_count(
    root_combat: &CombatState,
    plan: &TurnPlanV1,
) -> usize {
    let at_risk_lineages = root_combat
        .zones
        .hand
        .iter()
        .filter(|card| {
            cards::is_ethereal(card)
                && !card
                    .exhaust_override
                    .unwrap_or_else(|| cards::exhausts_when_played(card))
        })
        .map(|card| card.lineage);

    at_risk_lineages
        .filter(|lineage| {
            !plan
                .end_node
                .combat
                .zones
                .exhaust_pile
                .iter()
                .any(|card| card.lineage == *lineage)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::potions::{Potion, PotionId};
    use crate::runtime::combat::CombatCard;
    use crate::sim::combat::{
        combat_terminal, CombatPosition, CombatStepLimits, CombatStepResult, CombatTerminal,
    };
    use crate::state::core::ClientInput;
    use crate::test_support::{blank_test_combat, planned_monster, test_monster};

    #[derive(Clone, Copy)]
    struct SegmentTestStepper;

    impl CombatStepper for SegmentTestStepper {
        fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
            if !matches!(position.engine, EngineState::CombatPlayerTurn) {
                return Vec::new();
            }
            if position.combat.turn.energy > 0 && !position.combat.zones.hand.is_empty() {
                vec![
                    ClientInput::PlayCard {
                        card_index: 0,
                        target: Some(1),
                    },
                    ClientInput::EndTurn,
                ]
            } else {
                vec![ClientInput::EndTurn]
            }
        }

        fn apply_to_stable(
            &self,
            position: &CombatPosition,
            input: ClientInput,
            _limits: CombatStepLimits,
        ) -> CombatStepResult {
            let mut combat = position.combat.clone();
            if matches!(input, ClientInput::PlayCard { .. }) {
                combat.turn.energy = combat.turn.energy.saturating_sub(1);
                combat.zones.hand.clear();
                if let Some(monster) = combat.entities.monsters.first_mut() {
                    monster.current_hp = monster.current_hp.saturating_sub(12);
                }
            }
            if matches!(input, ClientInput::EndTurn) {
                combat.turn.turn_count = combat.turn.turn_count.saturating_add(1);
            }
            let position = CombatPosition::new(position.engine.clone(), combat);
            CombatStepResult {
                terminal: combat_terminal(&position.engine, &position.combat),
                alive: true,
                truncated: false,
                timed_out: false,
                engine_steps: 1,
                position,
            }
        }

        fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
            combat_terminal(&position.engine, &position.combat)
        }
    }

    #[test]
    fn turn_segment_report_is_partial_and_uses_current_turn_actions() {
        let mut combat = blank_test_combat();
        combat.entities.player.current_hp = 60;
        combat.entities.player.max_hp = 80;
        combat.turn.energy = 1;
        combat.turn.turn_count = 0;
        let mut monster = test_monster(EnemyId::JawWorm);
        monster.id = 1;
        monster.current_hp = 50;
        monster.max_hp = 50;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];

        let report = plan_combat_turn_segment_with_stepper_v1(
            &EngineState::CombatPlayerTurn,
            &combat,
            &CombatSearchV2Config::default(),
            &SegmentTestStepper,
        );

        assert_eq!(report.behavior_label, "partial_segment_not_terminal_claim");
        let selected = report.selected.expect("segment should be selected");
        assert_eq!(selected.terminal, SearchTerminalLabel::Unresolved);
        assert!(!selected.actions.is_empty());
        assert!(selected.actions.len() <= 2);
    }

    #[test]
    fn turn_segment_prefers_preserving_non_exhaust_ethereal_card_when_survival_is_equal() {
        let mut combat = blank_test_combat();
        combat.entities.player.current_hp = 60;
        combat.entities.player.max_hp = 80;
        combat.turn.energy = 2;
        combat.turn.turn_count = 0;
        let mut monster = planned_monster(EnemyId::JawWorm, 3);
        monster.id = 1;
        monster.current_hp = 50;
        monster.max_hp = 50;
        combat.entities.monsters = vec![monster];
        let mut defend = CombatCard::for_scenario(CardId::Defend, 1);
        defend.upgrades = 1;
        let mut ghostly = CombatCard::for_scenario(CardId::GhostlyArmor, 2);
        ghostly.upgrades = 1;
        combat.zones.hand = vec![defend, ghostly];

        let report = plan_combat_turn_segment_with_stepper_v1(
            &EngineState::CombatPlayerTurn,
            &combat,
            &CombatSearchV2Config::default(),
            &EngineCombatStepper,
        );

        let selected = report.selected.expect("segment should be selected");
        let played_ghostly = selected
            .actions
            .iter()
            .any(|action| matches!(action.input, ClientInput::PlayCard { card_index: 1, .. }));
        assert!(
            played_ghostly,
            "selected segment should preserve Ghostly Armor when hp result is already safe"
        );
    }

    #[test]
    fn turn_segment_inherits_potion_contract_when_survival_requires_it() {
        let mut combat = blank_test_combat();
        combat.meta.ascension_level = 20;
        combat.entities.player.current_hp = 10;
        combat.entities.player.max_hp = 80;
        combat.turn.energy = 0;
        let mut monster = test_monster(EnemyId::JawWorm);
        monster.current_hp = 40;
        monster.max_hp = 40;
        let plan = crate::content::monsters::roll_monster_turn_plan(
            &mut combat.rng.ai_rng,
            &monster,
            combat.meta.ascension_level,
            99,
            std::slice::from_ref(&monster),
            &[],
        );
        monster.set_planned_move_id(plan.move_id);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters = vec![monster];
        combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 7))];

        let no_potion = plan_combat_turn_segment_v1(
            &EngineState::CombatPlayerTurn,
            &combat,
            &CombatSearchV2Config::default(),
        );
        assert!(
            no_potion.selected.is_none(),
            "the unblocked Jaw Worm attack is lethal at this boundary"
        );

        let with_potion = plan_combat_turn_segment_v1(
            &EngineState::CombatPlayerTurn,
            &combat,
            &CombatSearchV2Config {
                potion_policy: super::super::CombatSearchV2PotionPolicy::SemanticBudgeted,
                max_potions_used: Some(1),
                ..CombatSearchV2Config::default()
            },
        );
        let selected = with_potion
            .selected
            .expect("the inherited potion contract should expose a surviving turn");
        assert!(selected.actions.iter().any(|action| {
            matches!(
                action.input,
                ClientInput::UsePotion {
                    potion_index: 0,
                    target: None
                }
            )
        }));
        assert_eq!(selected.final_hp, 10);
    }
}
