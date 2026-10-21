use serde::Serialize;

use crate::content::cards::{get_card_definition, CardId};
use crate::content::monsters::EnemyId;
use crate::content::potions::{get_potion_definition, PotionId};
use crate::content::powers::{get_power_definition, store, PowerId};
use crate::runtime::combat::{CardLineageId, CombatState};
use crate::sim::combat::{
    apply_combat_input_to_stable_observed_v1, CombatPosition, CombatStepLimits, CombatStepper,
    CombatTerminal, EngineCombatStepper,
};
use crate::sim::combat_projection::{project_monster_move_preview_in_combat, VisibleIntentKind};
use crate::state::core::ClientInput;
use crate::state::DomainCardSnapshot;

#[derive(Clone, Debug, Serialize)]
pub struct CombatWitnessStoryboardV1 {
    pub initial: CombatWitnessStoryboardBoundaryV1,
    pub turns: Vec<CombatWitnessStoryboardTurnV1>,
    pub cutpoints: Vec<CombatWitnessStoryboardCutpointV1>,
    pub final_boundary: CombatWitnessStoryboardBoundaryV1,
    pub total_actions: usize,
    pub terminal: CombatTerminal,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatWitnessStoryboardTurnV1 {
    pub turn: u32,
    pub first_action_index: usize,
    pub start: CombatWitnessStoryboardBoundaryV1,
    pub actions: Vec<CombatWitnessStoryboardActionV1>,
    pub end: CombatWitnessStoryboardBoundaryV1,
    pub observed_player_hp_delta: i32,
    pub observed_enemy_hp_delta: i32,
    pub self_transition_hp_loss: i32,
    pub end_turn_transition_hp_loss: i32,
    pub observed_hp_gain: i32,
    pub ended_with_unused_energy: Option<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatWitnessStoryboardActionV1 {
    pub action_index: usize,
    pub label: String,
    pub input: ClientInput,
    pub before_player_hp: i32,
    pub after_player_hp: i32,
    pub player_hp_delta: i32,
    pub before_player_block: i32,
    pub after_player_block: i32,
    pub before_energy: u8,
    pub after_energy: u8,
    pub before_visible_incoming_damage: i32,
    pub before_total_enemy_hp: i32,
    pub after_total_enemy_hp: i32,
    pub enemy_hp_delta: i32,
    pub drawn_cards: Vec<DomainCardSnapshot>,
    pub after_player_powers: Vec<CombatWitnessStoryboardPowerV1>,
    pub after_enemies: Vec<CombatWitnessStoryboardEnemyStateV1>,
    pub terminal: CombatTerminal,
    pub signals: Vec<CombatWitnessStoryboardSignalV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatWitnessStoryboardBoundaryV1 {
    pub turn: u32,
    pub terminal: CombatTerminal,
    pub player_hp: i32,
    pub player_max_hp: i32,
    pub player_block: i32,
    pub energy: u8,
    pub visible_incoming_damage: i32,
    pub player_powers: Vec<CombatWitnessStoryboardPowerV1>,
    pub hand: Vec<CombatWitnessStoryboardCardV1>,
    pub draw_count: usize,
    pub discard_count: usize,
    pub exhaust_count: usize,
    pub enemies: Vec<CombatWitnessStoryboardEnemyStateV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatWitnessStoryboardCardV1 {
    pub hand_index: usize,
    pub card_id: CardId,
    pub card_name: &'static str,
    pub upgrades: u8,
    pub cost_for_turn: i32,
    pub playable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatWitnessStoryboardEnemyStateV1 {
    pub monster_slot: usize,
    pub enemy_id: EnemyId,
    pub enemy_name: String,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub escaped: bool,
    pub visible_intent: VisibleIntentKind,
    pub visible_damage_per_hit: Option<i32>,
    pub visible_hits: u8,
    pub visible_total_damage: Option<i32>,
    pub powers: Vec<CombatWitnessStoryboardPowerV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatWitnessStoryboardPowerV1 {
    pub power_id: PowerId,
    pub power_name: &'static str,
    pub amount: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatWitnessStoryboardSignalV1 {
    SelfTransitionHpLoss,
    EndTurnTransitionHpLoss,
    UnusedEnergyAtEndTurn,
    PotionSpent,
    HpGain,
    TerminalHpGain,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatWitnessStoryboardCutpointV1 {
    pub action_index: usize,
    pub turn: u32,
    pub signal: CombatWitnessStoryboardSignalV1,
    pub observed_amount: i32,
    pub label: String,
}

pub fn build_combat_witness_storyboard_v1(
    root: &CombatPosition,
    inputs: &[ClientInput],
    max_engine_steps_per_action: usize,
) -> Result<CombatWitnessStoryboardV1, String> {
    let mut position = root.clone();
    let initial = storyboard_boundary(&position);
    let mut turns = Vec::<CombatWitnessStoryboardTurnV1>::new();
    let mut cutpoints = Vec::new();

    for (action_index, input) in inputs.iter().enumerate() {
        if EngineCombatStepper.terminal(&position) != CombatTerminal::Unresolved {
            return Err(format!(
                "combat storyboard continued after terminal action {action_index}"
            ));
        }
        if !EngineCombatStepper.is_legal_action(&position, input) {
            return Err(format!(
                "combat storyboard action {action_index} is not legal at its exact state"
            ));
        }
        let turn = position.combat.turn.turn_count;
        if turns.last().is_none_or(|current| current.turn != turn) {
            turns.push(CombatWitnessStoryboardTurnV1 {
                turn,
                first_action_index: action_index,
                start: storyboard_boundary(&position),
                actions: Vec::new(),
                end: storyboard_boundary(&position),
                observed_player_hp_delta: 0,
                observed_enemy_hp_delta: 0,
                self_transition_hp_loss: 0,
                end_turn_transition_hp_loss: 0,
                observed_hp_gain: 0,
                ended_with_unused_energy: None,
            });
        }

        let before = storyboard_boundary(&position);
        let label = action_label(&position, input);
        let observed = apply_combat_input_to_stable_observed_v1(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_action,
                deadline: None,
            },
        );
        if observed.step.timed_out || observed.step.truncated {
            return Err(format!(
                "combat storyboard action {action_index} did not reach a stable boundary"
            ));
        }
        let after = storyboard_boundary(&observed.step.position);
        let player_hp_delta = after.player_hp.saturating_sub(before.player_hp);
        let enemy_hp_delta = total_enemy_hp(&observed.step.position.combat)
            .saturating_sub(total_enemy_hp(&position.combat));
        let mut signals = Vec::new();
        if matches!(
            input,
            ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
        ) {
            signals.push(CombatWitnessStoryboardSignalV1::PotionSpent);
        }
        if player_hp_delta < 0 {
            signals.push(if matches!(input, ClientInput::EndTurn) {
                CombatWitnessStoryboardSignalV1::EndTurnTransitionHpLoss
            } else {
                CombatWitnessStoryboardSignalV1::SelfTransitionHpLoss
            });
        }
        if matches!(input, ClientInput::EndTurn) && before.energy > 0 {
            signals.push(CombatWitnessStoryboardSignalV1::UnusedEnergyAtEndTurn);
        }
        if player_hp_delta > 0 {
            signals.push(if observed.step.terminal == CombatTerminal::Win {
                CombatWitnessStoryboardSignalV1::TerminalHpGain
            } else {
                CombatWitnessStoryboardSignalV1::HpGain
            });
        }
        for signal in &signals {
            cutpoints.push(CombatWitnessStoryboardCutpointV1 {
                action_index,
                turn,
                signal: *signal,
                observed_amount: signal_amount(*signal, player_hp_delta, before.energy),
                label: label.clone(),
            });
        }
        let action = CombatWitnessStoryboardActionV1 {
            action_index,
            label,
            input: input.clone(),
            before_player_hp: before.player_hp,
            after_player_hp: after.player_hp,
            player_hp_delta,
            before_player_block: before.player_block,
            after_player_block: after.player_block,
            before_energy: before.energy,
            after_energy: after.energy,
            before_visible_incoming_damage: before.visible_incoming_damage,
            before_total_enemy_hp: total_enemy_hp(&position.combat),
            after_total_enemy_hp: total_enemy_hp(&observed.step.position.combat),
            enemy_hp_delta,
            drawn_cards: observed.drawn_cards,
            after_player_powers: after.player_powers.clone(),
            after_enemies: after.enemies.clone(),
            terminal: observed.step.terminal,
            signals,
        };
        let current = turns.last_mut().expect("current storyboard turn");
        current.observed_player_hp_delta = current
            .observed_player_hp_delta
            .saturating_add(player_hp_delta);
        current.observed_enemy_hp_delta = current
            .observed_enemy_hp_delta
            .saturating_add(enemy_hp_delta);
        if player_hp_delta < 0 {
            if matches!(input, ClientInput::EndTurn) {
                current.end_turn_transition_hp_loss = current
                    .end_turn_transition_hp_loss
                    .saturating_add(-player_hp_delta);
            } else {
                current.self_transition_hp_loss = current
                    .self_transition_hp_loss
                    .saturating_add(-player_hp_delta);
            }
        } else {
            current.observed_hp_gain = current.observed_hp_gain.saturating_add(player_hp_delta);
        }
        if matches!(input, ClientInput::EndTurn) {
            current.ended_with_unused_energy = (before.energy > 0).then_some(before.energy);
        }
        current.actions.push(action);
        current.end = after;
        position = observed.step.position;
    }

    let terminal = EngineCombatStepper.terminal(&position);
    Ok(CombatWitnessStoryboardV1 {
        initial,
        turns,
        cutpoints,
        final_boundary: storyboard_boundary(&position),
        total_actions: inputs.len(),
        terminal,
        notes: vec![
            "all rows come from one exact replay of the retained complete line",
            "HP changes are attributed to the selected stable transition, not claimed as a deeper causal decomposition of queued effects",
            "end-turn HP loss may combine visible enemy attacks with powers or other end-turn mechanics",
            "cutpoints are navigation hints for later local counterfactual work, not judgments that an action was wrong",
        ],
    })
}

fn storyboard_boundary(position: &CombatPosition) -> CombatWitnessStoryboardBoundaryV1 {
    let legal = EngineCombatStepper.atomic_actions(position);
    let mut playable_card_indices = legal
        .iter()
        .filter_map(|input| match input {
            ClientInput::PlayCard { card_index, .. } => Some(*card_index),
            _ => None,
        })
        .collect::<Vec<_>>();
    playable_card_indices.sort_unstable();
    playable_card_indices.dedup();
    let enemies = enemy_states(&position.combat);
    CombatWitnessStoryboardBoundaryV1 {
        turn: position.combat.turn.turn_count,
        terminal: EngineCombatStepper.terminal(position),
        player_hp: position.combat.entities.player.current_hp,
        player_max_hp: position.combat.entities.player.max_hp,
        player_block: position.combat.entities.player.block,
        energy: position.combat.turn.energy,
        visible_incoming_damage: enemies
            .iter()
            .filter(|enemy| enemy.alive)
            .filter_map(|enemy| enemy.visible_total_damage)
            .fold(0_i32, i32::saturating_add),
        player_powers: power_states(&position.combat, position.combat.entities.player.id),
        hand: position
            .combat
            .zones
            .hand
            .iter()
            .enumerate()
            .map(|(hand_index, card)| CombatWitnessStoryboardCardV1 {
                hand_index,
                card_id: card.id,
                card_name: get_card_definition(card.id).name,
                upgrades: card.upgrades,
                cost_for_turn: card.cost_for_turn_java(),
                playable: playable_card_indices.binary_search(&hand_index).is_ok(),
            })
            .collect(),
        draw_count: position.combat.zones.draw_pile.len(),
        discard_count: position.combat.zones.discard_pile.len(),
        exhaust_count: position.combat.zones.exhaust_pile.len(),
        enemies,
    }
}

fn enemy_states(combat: &CombatState) -> Vec<CombatWitnessStoryboardEnemyStateV1> {
    combat
        .entities
        .monsters
        .iter()
        .enumerate()
        .map(|(monster_slot, monster)| {
            let enemy_id = monster.enemy_id;
            let preview = project_monster_move_preview_in_combat(combat, monster);
            CombatWitnessStoryboardEnemyStateV1 {
                monster_slot,
                enemy_id,
                enemy_name: enemy_id.get_name().to_string(),
                hp: monster.current_hp,
                max_hp: monster.max_hp,
                block: monster.block,
                alive: monster.is_alive_for_action(),
                escaped: monster.is_escaped,
                visible_intent: preview.visible_intent,
                visible_damage_per_hit: preview.damage_per_hit,
                visible_hits: preview.hits,
                visible_total_damage: preview.total_damage,
                powers: power_states(combat, monster.id),
            }
        })
        .collect()
}

fn power_states(
    combat: &CombatState,
    entity: crate::EntityId,
) -> Vec<CombatWitnessStoryboardPowerV1> {
    store::powers_for(combat, entity)
        .unwrap_or_default()
        .iter()
        .map(|power| CombatWitnessStoryboardPowerV1 {
            power_id: power.power_type,
            power_name: get_power_definition(power.power_type).name,
            amount: power.amount,
        })
        .collect()
}

fn action_label(position: &CombatPosition, input: &ClientInput) -> String {
    match input {
        ClientInput::PlayCard { card_index, target } => {
            let card = position.combat.zones.hand.get(*card_index);
            let card_label = card
                .map(|card| {
                    format!(
                        "{}{}",
                        get_card_definition(card.id).name,
                        if card.upgrades > 0 { "+" } else { "" }
                    )
                })
                .unwrap_or_else(|| format!("hand[{card_index}]"));
            format!(
                "play {card_label}{}",
                target_label(&position.combat, *target)
            )
        }
        ClientInput::UsePotion {
            potion_index,
            target,
        } => {
            let potion = position
                .combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .map(|potion| potion.id);
            format!(
                "use {}{}",
                potion
                    .map(potion_name)
                    .unwrap_or_else(|| format!("potion[{potion_index}]")),
                target_label(&position.combat, *target)
            )
        }
        ClientInput::DiscardPotion(slot) => {
            let potion = position
                .combat
                .entities
                .potions
                .get(*slot)
                .and_then(Option::as_ref)
                .map(|potion| potion.id);
            format!(
                "discard {}",
                potion
                    .map(potion_name)
                    .unwrap_or_else(|| format!("potion[{slot}]"))
            )
        }
        ClientInput::EndTurn => "end turn".to_string(),
        ClientInput::SubmitSelection(selection) => {
            let selected = selection
                .selected_card_lineages()
                .iter()
                .map(|lineage| selected_card_label(&position.combat, *lineage))
                .collect::<Vec<_>>()
                .join(", ");
            format!("select {selected}")
        }
        _ => format!("{:?}", input),
    }
}

fn selected_card_label(combat: &CombatState, lineage: CardLineageId) -> String {
    let zones = &combat.zones;
    zones
        .hand
        .iter()
        .chain(zones.draw_pile.iter())
        .chain(zones.discard_pile.iter())
        .chain(zones.exhaust_pile.iter())
        .chain(zones.limbo.iter())
        .find(|card| card.lineage == lineage)
        .map(|card| {
            format!(
                "{}{} [lineage={lineage}]",
                get_card_definition(card.id).name,
                if card.upgrades > 0 { "+" } else { "" }
            )
        })
        .unwrap_or_else(|| format!("card lineage={lineage}"))
}

fn target_label(combat: &CombatState, target: Option<crate::EntityId>) -> String {
    let Some(target) = target else {
        return String::new();
    };
    combat
        .entities
        .monsters
        .iter()
        .enumerate()
        .find(|(_, monster)| monster.id == target)
        .map(|(slot, monster)| {
            let name = monster.enemy_id.get_name().to_string();
            format!(" -> {name}[{slot}]")
        })
        .unwrap_or_else(|| format!(" -> entity#{target}"))
}

fn potion_name(id: PotionId) -> String {
    get_potion_definition(id).name.to_string()
}

fn total_enemy_hp(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.current_hp.max(0))
        .fold(0_i32, i32::saturating_add)
}

fn signal_amount(
    signal: CombatWitnessStoryboardSignalV1,
    player_hp_delta: i32,
    before_energy: u8,
) -> i32 {
    match signal {
        CombatWitnessStoryboardSignalV1::SelfTransitionHpLoss
        | CombatWitnessStoryboardSignalV1::EndTurnTransitionHpLoss => -player_hp_delta,
        CombatWitnessStoryboardSignalV1::UnusedEnergyAtEndTurn => before_energy.into(),
        CombatWitnessStoryboardSignalV1::PotionSpent => 1,
        CombatWitnessStoryboardSignalV1::HpGain
        | CombatWitnessStoryboardSignalV1::TerminalHpGain => player_hp_delta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{EngineState, RunResult};
    use crate::test_support::{blank_test_combat, test_monster};

    #[test]
    fn storyboard_names_the_selected_card_and_marks_self_loss() {
        let mut combat = blank_test_combat();
        combat.entities.player.current_hp = 20;
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        combat.entities.monsters[0].current_hp = 1;
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Offering, 1)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let actions = vec![ClientInput::PlayCard {
            card_index: 0,
            target: None,
        }];

        // Offering alone does not end this synthetic fight, but it gives us a
        // deterministic stable transition with player HP loss.
        let board = build_combat_witness_storyboard_v1(&root, &actions, 250)
            .expect("storyboard exact replay");

        assert_eq!(board.turns[0].actions[0].label, "play Offering");
        assert!(board.turns[0].actions[0]
            .signals
            .contains(&CombatWitnessStoryboardSignalV1::SelfTransitionHpLoss));
    }

    #[test]
    fn storyboard_accepts_a_terminal_line() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        combat.entities.monsters[0].current_hp = 1;
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let board = build_combat_witness_storyboard_v1(
            &root,
            &[ClientInput::PlayCard {
                card_index: 0,
                target: Some(root.combat.entities.monsters[0].id),
            }],
            250,
        )
        .expect("terminal storyboard exact replay");

        assert!(matches!(
            board.terminal,
            CombatTerminal::Win | CombatTerminal::Unresolved
        ));
        assert_eq!(board.final_boundary.visible_incoming_damage, 0);
        assert_ne!(EngineState::GameOver(RunResult::Defeat), root.engine);
    }
}
