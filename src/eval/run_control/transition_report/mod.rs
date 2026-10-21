use std::collections::HashMap;

use crate::content::cards::get_card_definition;
use crate::content::identity::gameplay_display_label_v1;
use crate::content::potions::{get_potion_definition, PotionId};
use crate::content::relics::RelicId;
use crate::runtime::combat::CombatCard;
use crate::state::core::ClientInput;
use crate::state::selection::DomainEvent;

use super::session::RunControlSession;
use super::view_model::{build_run_control_view_model, client_input_hint};
mod types;

pub use types::{
    ActionResult, ActionResultChange, CardSnapshot, CombatPlayerResult, MonsterSnapshot,
    PileCounts, RunApplyStatus, RunEndResult, RunKey, ValueChange,
};
use types::{CombatSnapshot, PotionSnapshot, RelicSnapshot};
pub(super) use types::{RunVisibleSnapshot, TransitionAction};

pub(super) fn transition_action_for_input(
    session: &RunControlSession,
    input: &ClientInput,
) -> TransitionAction {
    let view = build_run_control_view_model(session);
    let label = view
        .candidates
        .iter()
        .find(|candidate| candidate.action.executable_input().as_ref() == Some(input))
        .map(|candidate| candidate.label.clone())
        .unwrap_or_else(|| client_input_hint(input));
    TransitionAction { label }
}

#[cfg(test)]
fn render_transition_report(
    action: TransitionAction,
    before: &RunVisibleSnapshot,
    after: &RunVisibleSnapshot,
    status: RunApplyStatus,
) -> String {
    let result = action_result_from_transition(action, before, after, status);
    render_action_result(&result)
}

pub(super) fn action_result_from_transition(
    action: TransitionAction,
    before: &RunVisibleSnapshot,
    after: &RunVisibleSnapshot,
    status: RunApplyStatus,
) -> ActionResult {
    let mut changes = Vec::new();
    push_resource_delta(before, after, &mut changes);
    push_relic_delta(before, after, &mut changes);
    push_potion_delta(before, after, &mut changes);
    push_deck_delta(before, after, &mut changes);
    push_key_delta(before, after, &mut changes);
    push_combat_delta(before, after, &mut changes);
    if before.act != after.act || before.floor != after.floor {
        changes.push(ActionResultChange::LocationChanged {
            before_act: before.act,
            before_floor: before.floor,
            after_act: after.act,
            after_floor: after.floor,
        });
    }
    if before.title != after.title {
        changes.push(ActionResultChange::AdvancedTo {
            title: after.title.clone(),
        });
    }
    match status {
        RunApplyStatus::Running => {}
        RunApplyStatus::Victory => changes.push(ActionResultChange::RunEnded {
            result: RunEndResult::Victory,
        }),
        RunApplyStatus::Defeat => changes.push(ActionResultChange::RunEnded {
            result: RunEndResult::Defeat,
        }),
        RunApplyStatus::Stopped => changes.push(ActionResultChange::EngineStopped),
    }
    if changes.is_empty() {
        changes.push(ActionResultChange::NoVisibleStateChanges);
    }
    ActionResult {
        chosen_label: action.label,
        status,
        changes,
    }
}

pub(super) fn action_result_from_transition_with_extra_changes(
    action: TransitionAction,
    before: &RunVisibleSnapshot,
    after: &RunVisibleSnapshot,
    status: RunApplyStatus,
    mut extra_changes: Vec<ActionResultChange>,
) -> ActionResult {
    let mut result = action_result_from_transition(action, before, after, status);
    if !extra_changes.is_empty() {
        if matches!(
            result.changes.as_slice(),
            [ActionResultChange::NoVisibleStateChanges]
        ) {
            result.changes.clear();
        }
        result.changes.append(&mut extra_changes);
    }
    result
}

pub(super) fn action_result_changes_from_domain_events(
    events: Vec<DomainEvent>,
) -> Vec<ActionResultChange> {
    events
        .into_iter()
        .filter_map(|event| match event {
            DomainEvent::CardDrawn { card, .. } => Some(ActionResultChange::CombatCardDrawn {
                card: CardSnapshot {
                    id: card.id,
                    lineage: card.lineage,
                    upgrades: card.upgrades,
                },
            }),
            _ => None,
        })
        .collect()
}

pub(super) fn render_action_result(result: &ActionResult) -> String {
    let mut lines = Vec::new();
    lines.push("Result:".to_string());
    lines.push(format!("  Chose: {}", result.chosen_label));
    for change in &result.changes {
        render_action_result_change(change, &mut lines);
    }
    if has_relic_result_change(result) {
        lines.push("  Type `relics` to inspect current relics.".to_string());
    }
    lines.join("\n")
}

fn has_relic_result_change(result: &ActionResult) -> bool {
    result.changes.iter().any(|change| {
        matches!(
            change,
            ActionResultChange::RelicGained { .. }
                | ActionResultChange::RelicLost { .. }
                | ActionResultChange::RelicChanged { .. }
        )
    })
}

impl RunVisibleSnapshot {
    pub(super) fn capture(session: &RunControlSession) -> Self {
        let view = build_run_control_view_model(session);
        let (current_hp, max_hp) = session.visible_player_hp();
        Self {
            title: view.header.title,
            current_hp,
            max_hp,
            gold: session.run_state.gold,
            act: session.run_state.act_num,
            floor: session.run_state.floor_num,
            keys: session.run_state.keys,
            relics: session
                .run_state
                .relics
                .iter()
                .map(|relic| RelicSnapshot {
                    id: relic.id,
                    counter: relic.counter,
                    used_up: relic.used_up,
                    amount: relic.amount,
                })
                .collect(),
            potions: session
                .visible_potions()
                .iter()
                .map(|slot| {
                    slot.as_ref().map(|potion| PotionSnapshot {
                        id: potion.id,
                        uuid: potion.uuid,
                    })
                })
                .collect(),
            deck: session
                .run_state
                .master_deck
                .iter()
                .map(card_snapshot)
                .collect(),
            combat: session.active_combat.as_ref().map(|active| {
                let combat = &active.combat_state;
                CombatSnapshot {
                    player_hp: combat.entities.player.current_hp,
                    player_max_hp: combat.entities.player.max_hp,
                    player_block: combat.entities.player.block,
                    energy: i32::from(combat.turn.energy),
                    monsters: combat
                        .entities
                        .monsters
                        .iter()
                        .map(|monster| MonsterSnapshot {
                            id: monster.id,
                            label: super::view_model::monster_name(monster.enemy_id),
                            hp: monster.current_hp,
                            max_hp: monster.max_hp,
                            block: monster.block,
                            alive: monster.is_alive_for_action(),
                        })
                        .collect(),
                    hand_count: combat.zones.hand.len(),
                    draw_count: combat.zones.draw_pile.len(),
                    discard_count: combat.zones.discard_pile.len(),
                    exhaust_count: combat.zones.exhaust_pile.len(),
                }
            }),
        }
    }
}

impl CombatSnapshot {
    fn player(&self) -> CombatPlayerResult {
        CombatPlayerResult {
            hp: self.player_hp,
            max_hp: self.player_max_hp,
            block: self.player_block,
            energy: self.energy,
        }
    }

    fn piles(&self) -> PileCounts {
        PileCounts {
            hand: self.hand_count,
            draw: self.draw_count,
            discard: self.discard_count,
            exhaust: self.exhaust_count,
        }
    }
}

fn card_snapshot(card: &CombatCard) -> CardSnapshot {
    CardSnapshot {
        id: card.id,
        lineage: card.lineage,
        upgrades: card.upgrades,
    }
}

fn render_action_result_change(change: &ActionResultChange, lines: &mut Vec<String>) {
    match change {
        ActionResultChange::HpChanged {
            before_current,
            before_max,
            after_current,
            after_max,
        } => lines.push(format!(
            "  HP: {before_current}/{before_max} -> {after_current}/{after_max}"
        )),
        ActionResultChange::GoldChanged { before, after } => {
            lines.push(format!("  Gold: {before} -> {after}"));
        }
        ActionResultChange::RelicGained { relic } => {
            lines.push(format!("  Gained relic: {}", relic_label(*relic)));
        }
        ActionResultChange::RelicLost { relic } => {
            lines.push(format!("  Lost relic: {}", relic_label(*relic)));
        }
        ActionResultChange::RelicChanged {
            relic,
            counter,
            amount,
            used_up,
        } => {
            let mut parts = Vec::new();
            if let Some(counter) = counter {
                parts.push(format!("counter {} -> {}", counter.before, counter.after));
            }
            if let Some(amount) = amount {
                parts.push(format!("amount {} -> {}", amount.before, amount.after));
            }
            if let Some(used_up) = used_up {
                parts.push(format!("used {} -> {}", used_up.before, used_up.after));
            }
            lines.push(format!(
                "  Relic changed: {} ({})",
                relic_label(*relic),
                parts.join(", ")
            ));
        }
        ActionResultChange::PotionGained { potion, slot } => {
            lines.push(format!(
                "  Gained potion: {} in slot {}",
                potion_label(*potion),
                slot
            ));
        }
        ActionResultChange::PotionLost { potion, slot } => {
            lines.push(format!(
                "  Lost potion: {} from slot {}",
                potion_label(*potion),
                slot
            ));
        }
        ActionResultChange::PotionChanged {
            slot,
            before,
            after,
        } => {
            lines.push(format!(
                "  Potion slot {}: {} -> {}",
                slot,
                potion_label(*before),
                potion_label(*after)
            ));
        }
        ActionResultChange::CardRemoved { card } => {
            lines.push(format!("  Removed card: {}", card_label(card)));
        }
        ActionResultChange::CardAdded { card } => {
            lines.push(format!("  Added card: {}", card_label(card)));
        }
        ActionResultChange::CardTransformed { before, after } => {
            lines.push(format!(
                "  Transformed card: {} -> {}",
                card_label(before),
                card_label(after)
            ));
        }
        ActionResultChange::CardUpgraded { before, after } => {
            lines.push(format!(
                "  Upgraded card: {} -> {}",
                card_label(before),
                card_label(after)
            ));
        }
        ActionResultChange::KeyChanged { key, obtained } => {
            lines.push(format!(
                "  {} key: {}",
                key_label(*key),
                if *obtained { "obtained" } else { "lost" }
            ));
        }
        ActionResultChange::CombatStarted { player, monsters } => {
            lines.push("  Combat started.".to_string());
            lines.push(format!(
                "  Player: HP {}/{} Block {} Energy {}",
                player.hp, player.max_hp, player.block, player.energy
            ));
            for monster in monsters {
                lines.push(format!(
                    "  Enemy: {} HP {}/{} Block {}",
                    monster.label, monster.hp, monster.max_hp, monster.block
                ));
            }
        }
        ActionResultChange::CombatEnded => lines.push("  Combat ended.".to_string()),
        ActionResultChange::CombatCardDrawn { card } => {
            lines.push(format!("  Drew card: {}", card_label(card)));
        }
        ActionResultChange::CombatPlayerChanged { before, after } => {
            lines.push(format!(
                "  Player: HP {}/{} -> {}/{} | Block {} -> {} | Energy {} -> {}",
                before.hp,
                before.max_hp,
                after.hp,
                after.max_hp,
                before.block,
                after.block,
                before.energy,
                after.energy
            ));
        }
        ActionResultChange::CombatMonsterChanged { before, after } => {
            lines.push(format!(
                "  Enemy {}: HP {}/{} -> {}/{} | Block {} -> {} | Alive {} -> {}",
                after.label,
                before.hp,
                before.max_hp,
                after.hp,
                after.max_hp,
                before.block,
                after.block,
                before.alive,
                after.alive
            ));
        }
        ActionResultChange::PileCountsChanged { before, after } => {
            lines.push(format!(
                "  Piles: hand {} -> {}, draw {} -> {}, discard {} -> {}, exhaust {} -> {}",
                before.hand,
                after.hand,
                before.draw,
                after.draw,
                before.discard,
                after.discard,
                before.exhaust,
                after.exhaust
            ));
        }
        ActionResultChange::LocationChanged {
            before_act,
            before_floor,
            after_act,
            after_floor,
        } => {
            lines.push(format!(
                "  Location: Act {before_act} Floor {before_floor} -> Act {after_act} Floor {after_floor}"
            ));
        }
        ActionResultChange::AdvancedTo { title } => {
            lines.push(format!("  Advanced to: {title}"));
        }
        ActionResultChange::RunEnded { result } => {
            lines.push(format!("  Run ended: {}", run_end_label(*result)));
        }
        ActionResultChange::EngineStopped => lines.push("  Engine stopped".to_string()),
        ActionResultChange::NoVisibleStateChanges => {
            lines.push("  No visible state changes.".to_string());
        }
    }
}

fn value_change<T: Copy + PartialEq>(before: T, after: T) -> Option<ValueChange<T>> {
    (before != after).then_some(ValueChange { before, after })
}

fn push_resource_delta(
    before: &RunVisibleSnapshot,
    after: &RunVisibleSnapshot,
    changes: &mut Vec<ActionResultChange>,
) {
    if before.current_hp != after.current_hp || before.max_hp != after.max_hp {
        changes.push(ActionResultChange::HpChanged {
            before_current: before.current_hp,
            before_max: before.max_hp,
            after_current: after.current_hp,
            after_max: after.max_hp,
        });
    }
    if before.gold != after.gold {
        changes.push(ActionResultChange::GoldChanged {
            before: before.gold,
            after: after.gold,
        });
    }
}

fn push_relic_delta(
    before: &RunVisibleSnapshot,
    after: &RunVisibleSnapshot,
    changes: &mut Vec<ActionResultChange>,
) {
    let before_counts = relic_counts(&before.relics);
    let after_counts = relic_counts(&after.relics);
    let mut ids = before_counts
        .keys()
        .chain(after_counts.keys())
        .copied()
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| relic_label(*id));
    ids.dedup();
    for id in ids {
        let old = before_counts.get(&id).copied().unwrap_or(0);
        let new = after_counts.get(&id).copied().unwrap_or(0);
        if new > old {
            for _ in 0..(new - old) {
                changes.push(ActionResultChange::RelicGained { relic: id });
            }
        } else if old > new {
            for _ in 0..(old - new) {
                changes.push(ActionResultChange::RelicLost { relic: id });
            }
        }
    }

    for after_relic in &after.relics {
        if let Some(before_relic) = before
            .relics
            .iter()
            .find(|before_relic| before_relic.id == after_relic.id)
        {
            let counter = value_change(before_relic.counter, after_relic.counter);
            let amount = value_change(before_relic.amount, after_relic.amount);
            let used_up = value_change(before_relic.used_up, after_relic.used_up);
            if counter.is_some() || amount.is_some() || used_up.is_some() {
                changes.push(ActionResultChange::RelicChanged {
                    relic: after_relic.id,
                    counter,
                    amount,
                    used_up,
                });
            }
        }
    }
}

fn push_potion_delta(
    before: &RunVisibleSnapshot,
    after: &RunVisibleSnapshot,
    changes: &mut Vec<ActionResultChange>,
) {
    let max_len = before.potions.len().max(after.potions.len());
    for slot in 0..max_len {
        let old = before.potions.get(slot).and_then(|value| value.as_ref());
        let new = after.potions.get(slot).and_then(|value| value.as_ref());
        match (old, new) {
            (None, Some(potion)) => changes.push(ActionResultChange::PotionGained {
                potion: potion.id,
                slot,
            }),
            (Some(potion), None) => changes.push(ActionResultChange::PotionLost {
                potion: potion.id,
                slot,
            }),
            (Some(old), Some(new)) if old.id != new.id || old.uuid != new.uuid => {
                changes.push(ActionResultChange::PotionChanged {
                    slot,
                    before: old.id,
                    after: new.id,
                });
            }
            _ => {}
        }
    }
}

fn push_deck_delta(
    before: &RunVisibleSnapshot,
    after: &RunVisibleSnapshot,
    changes: &mut Vec<ActionResultChange>,
) {
    let before_by_lineage = before
        .deck
        .iter()
        .map(|card| (card.lineage, card))
        .collect::<HashMap<_, _>>();
    let after_by_lineage = after
        .deck
        .iter()
        .map(|card| (card.lineage, card))
        .collect::<HashMap<_, _>>();

    for card in &before.deck {
        if !after_by_lineage.contains_key(&card.lineage) {
            changes.push(ActionResultChange::CardRemoved { card: card.clone() });
        }
    }
    for card in &after.deck {
        match before_by_lineage.get(&card.lineage) {
            None => changes.push(ActionResultChange::CardAdded { card: card.clone() }),
            Some(before_card) if before_card.id != card.id => {
                changes.push(ActionResultChange::CardTransformed {
                    before: (*before_card).clone(),
                    after: card.clone(),
                });
            }
            Some(before_card) if before_card.upgrades != card.upgrades => {
                changes.push(ActionResultChange::CardUpgraded {
                    before: (*before_card).clone(),
                    after: card.clone(),
                });
            }
            _ => {}
        }
    }
}

fn push_key_delta(
    before: &RunVisibleSnapshot,
    after: &RunVisibleSnapshot,
    changes: &mut Vec<ActionResultChange>,
) {
    for (idx, (old, new)) in before.keys.iter().zip(after.keys.iter()).enumerate() {
        if old != new {
            changes.push(ActionResultChange::KeyChanged {
                key: run_key(idx),
                obtained: *new,
            });
        }
    }
}

fn push_combat_delta(
    before: &RunVisibleSnapshot,
    after: &RunVisibleSnapshot,
    changes: &mut Vec<ActionResultChange>,
) {
    match (&before.combat, &after.combat) {
        (None, Some(after_combat)) => {
            changes.push(ActionResultChange::CombatStarted {
                player: after_combat.player(),
                monsters: after_combat.monsters.clone(),
            });
        }
        (Some(_), None) => changes.push(ActionResultChange::CombatEnded),
        (Some(before_combat), Some(after_combat)) => {
            if before_combat.player_hp != after_combat.player_hp
                || before_combat.player_block != after_combat.player_block
                || before_combat.energy != after_combat.energy
            {
                changes.push(ActionResultChange::CombatPlayerChanged {
                    before: before_combat.player(),
                    after: after_combat.player(),
                });
            }
            for after_monster in &after_combat.monsters {
                if let Some(before_monster) = before_combat
                    .monsters
                    .iter()
                    .find(|monster| monster.id == after_monster.id)
                {
                    if before_monster.hp != after_monster.hp
                        || before_monster.block != after_monster.block
                        || before_monster.alive != after_monster.alive
                    {
                        changes.push(ActionResultChange::CombatMonsterChanged {
                            before: before_monster.clone(),
                            after: after_monster.clone(),
                        });
                    }
                }
            }
            if before_combat.hand_count != after_combat.hand_count
                || before_combat.draw_count != after_combat.draw_count
                || before_combat.discard_count != after_combat.discard_count
                || before_combat.exhaust_count != after_combat.exhaust_count
            {
                changes.push(ActionResultChange::PileCountsChanged {
                    before: before_combat.piles(),
                    after: after_combat.piles(),
                });
            }
        }
        (None, None) => {}
    }
}

fn relic_counts(relics: &[RelicSnapshot]) -> HashMap<RelicId, usize> {
    let mut counts = HashMap::new();
    for relic in relics {
        *counts.entry(relic.id).or_insert(0) += 1;
    }
    counts
}

fn card_label(card: &CardSnapshot) -> String {
    let name = get_card_definition(card.id).name;
    if card.upgrades == 0 {
        name.to_string()
    } else {
        format!("{name}+{}", card.upgrades)
    }
}

fn potion_label(id: PotionId) -> &'static str {
    get_potion_definition(id).name
}

fn relic_label(id: RelicId) -> String {
    gameplay_display_label_v1(id).to_string()
}

fn run_key(idx: usize) -> RunKey {
    match idx {
        0 => RunKey::Ruby,
        1 => RunKey::Sapphire,
        2 => RunKey::Emerald,
        _ => RunKey::Emerald,
    }
}

fn key_label(key: RunKey) -> &'static str {
    match key {
        RunKey::Ruby => "Ruby",
        RunKey::Sapphire => "Sapphire",
        RunKey::Emerald => "Emerald",
    }
}

fn run_end_label(result: RunEndResult) -> &'static str {
    match result {
        RunEndResult::Victory => "victory",
        RunEndResult::Defeat => "defeat",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicState;
    use crate::eval::run_control::session::{RunControlConfig, RunControlSession};

    #[test]
    fn transition_report_renders_relic_swap_from_state_diff() {
        let before = RunVisibleSnapshot {
            title: "Neow Bonus".to_string(),
            current_hp: 80,
            max_hp: 80,
            gold: 99,
            act: 1,
            floor: 0,
            keys: [false; 3],
            relics: vec![RelicSnapshot {
                id: RelicId::BurningBlood,
                counter: -1,
                used_up: false,
                amount: 0,
            }],
            potions: vec![None, None, None],
            deck: Vec::new(),
            combat: None,
        };
        let after = RunVisibleSnapshot {
            relics: vec![RelicSnapshot {
                id: RelicId::Astrolabe,
                counter: -1,
                used_up: false,
                amount: 0,
            }],
            ..before.clone()
        };
        let rendered = render_transition_report(
            TransitionAction {
                label: "Boss relic swap".to_string(),
            },
            &before,
            &after,
            RunApplyStatus::Running,
        );

        assert!(rendered.contains("Lost relic: Burning Blood"));
        assert!(rendered.contains("Gained relic: Astrolabe"));
        assert!(!rendered.contains("ok"));
    }

    #[test]
    fn action_result_keeps_typed_relic_swap_changes() {
        let before = RunVisibleSnapshot {
            title: "Neow Bonus".to_string(),
            current_hp: 80,
            max_hp: 80,
            gold: 99,
            act: 1,
            floor: 0,
            keys: [false; 3],
            relics: vec![RelicSnapshot {
                id: RelicId::BurningBlood,
                counter: -1,
                used_up: false,
                amount: 0,
            }],
            potions: vec![None, None, None],
            deck: Vec::new(),
            combat: None,
        };
        let after = RunVisibleSnapshot {
            relics: vec![RelicSnapshot {
                id: RelicId::Astrolabe,
                counter: -1,
                used_up: false,
                amount: 0,
            }],
            ..before.clone()
        };

        let result = action_result_from_transition(
            TransitionAction {
                label: "Boss relic swap".to_string(),
            },
            &before,
            &after,
            RunApplyStatus::Running,
        );

        assert!(result.changes.iter().any(|change| matches!(
            change,
            ActionResultChange::RelicGained {
                relic: RelicId::Astrolabe
            }
        )));
        assert!(result.changes.iter().any(|change| matches!(
            change,
            ActionResultChange::RelicLost {
                relic: RelicId::BurningBlood
            }
        )));
        let json = serde_json::to_string(&result).expect("action result should serialize");
        assert!(json.contains("relic_gained"));
        assert!(json.contains("relic_lost"));
    }

    #[test]
    fn relic_result_hint_only_appears_for_relic_changes() {
        let relic_result = ActionResult {
            chosen_label: "Take relic".to_string(),
            status: RunApplyStatus::Running,
            changes: vec![ActionResultChange::RelicGained {
                relic: RelicId::PeacePipe,
            }],
        };
        let card_result = ActionResult {
            chosen_label: "Take card".to_string(),
            status: RunApplyStatus::Running,
            changes: vec![ActionResultChange::CardAdded {
                card: CardSnapshot {
                    id: crate::content::cards::CardId::Clothesline,
                    lineage: crate::test_support::scenario_lineage(1),
                    upgrades: 0,
                },
            }],
        };

        let relic_rendered = render_action_result(&relic_result);
        let card_rendered = render_action_result(&card_result);

        assert!(relic_rendered.contains("Type `relics` to inspect current relics."));
        assert!(!card_rendered.contains("Type `relics` to inspect current relics."));
    }

    #[test]
    fn action_result_detects_same_id_potion_replacement_by_lineage() {
        let before = RunVisibleSnapshot {
            title: "Combat".to_string(),
            current_hp: 80,
            max_hp: 80,
            gold: 99,
            act: 1,
            floor: 1,
            keys: [false; 3],
            relics: Vec::new(),
            potions: vec![Some(PotionSnapshot {
                id: PotionId::EntropicBrew,
                uuid: 101,
            })],
            deck: Vec::new(),
            combat: None,
        };
        let after = RunVisibleSnapshot {
            potions: vec![Some(PotionSnapshot {
                id: PotionId::EntropicBrew,
                uuid: 40_010,
            })],
            ..before.clone()
        };

        let result = action_result_from_transition(
            TransitionAction {
                label: "Use Entropic Brew".to_string(),
            },
            &before,
            &after,
            RunApplyStatus::Running,
        );

        assert!(result.changes.iter().any(|change| matches!(
            change,
            ActionResultChange::PotionChanged {
                slot: 0,
                before: PotionId::EntropicBrew,
                after: PotionId::EntropicBrew,
            }
        )));
    }

    #[test]
    fn transition_report_captures_neow_gold_delta() {
        let mut session = RunControlSession::new(RunControlConfig {
            seed: 521,
            ..RunControlConfig::default()
        });
        session
            .apply_only_candidate()
            .expect("Neow intro should advance");
        let before = RunVisibleSnapshot::capture(&session);
        let action = transition_action_for_input(&session, &ClientInput::EventChoice(1));
        session
            .apply_candidate_id("1")
            .expect("gold option should execute");
        let after = RunVisibleSnapshot::capture(&session);
        let rendered = render_transition_report(action, &before, &after, RunApplyStatus::Running);

        assert!(rendered.contains("Chose: Obtain 100 Gold."));
        assert!(rendered.contains("Gold: 99 -> 199"));
    }

    #[test]
    fn snapshot_records_relics_without_consuming_run_events() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.relics.clear();
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::Anchor));
        session.run_state.emitted_events.push(
            crate::state::selection::DomainEvent::RelicObtained {
                relic_id: RelicId::Anchor,
                source: crate::state::selection::DomainEventSource::RewardScreen,
            },
        );

        let _snapshot = RunVisibleSnapshot::capture(&session);

        assert_eq!(session.run_state.emitted_events.len(), 1);
    }
}
