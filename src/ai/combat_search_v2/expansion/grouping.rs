use std::collections::BTreeMap;

use super::super::*;
use super::types::{
    ActionExpansionGroupKey, ActionExpansionGroupSummary, ActionExpansionKind,
    ActionExpansionSummary,
};
use crate::state::selection::SelectionScope;

pub(in crate::ai::combat_search_v2) fn summarize_action_expansion(
    engine: &EngineState,
    combat: &CombatState,
    choices: &[CombatActionChoice],
) -> ActionExpansionSummary {
    let mut groups: BTreeMap<ActionExpansionGroupKey, usize> = BTreeMap::new();
    for choice in choices {
        let key = group_key_for_input(engine, combat, &choice.input);
        *groups.entry(key).or_insert(0) += 1;
    }

    let groups = groups
        .into_iter()
        .map(|(key, action_count)| ActionExpansionGroupSummary { key, action_count })
        .collect::<Vec<_>>();

    ActionExpansionSummary {
        action_count: choices.len(),
        group_count: groups.len(),
        groups,
    }
}

fn group_key_for_input(
    engine: &EngineState,
    combat: &CombatState,
    input: &ClientInput,
) -> ActionExpansionGroupKey {
    match input {
        ClientInput::PlayCard { card_index, .. } => ActionExpansionGroupKey {
            kind: ActionExpansionKind::PlayCard,
            signature: combat
                .zones
                .hand
                .get(*card_index)
                .map(|card| {
                    format!(
                        "play_card/hand:{card_index}/card:{}+{}/lineage:{}/cost:{}",
                        card.id.stable_id(),
                        card.upgrades,
                        card.lineage,
                        card.cost_for_turn_java()
                    )
                })
                .unwrap_or_else(|| format!("play_card/hand:{card_index}/missing_card")),
        },
        ClientInput::EndTurn => ActionExpansionGroupKey {
            kind: ActionExpansionKind::EndTurn,
            signature: "end_turn".to_string(),
        },
        ClientInput::UsePotion { potion_index, .. } => ActionExpansionGroupKey {
            kind: ActionExpansionKind::UsePotion,
            signature: combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .map(|potion| {
                    format!(
                        "use_potion/slot:{potion_index}/potion:{:?}/uuid:{}",
                        potion.id, potion.uuid
                    )
                })
                .unwrap_or_else(|| format!("use_potion/slot:{potion_index}/missing_potion")),
        },
        ClientInput::DiscardPotion(slot) => ActionExpansionGroupKey {
            kind: ActionExpansionKind::DiscardPotion,
            signature: format!("discard_potion/slot:{slot}"),
        },
        ClientInput::SubmitDiscoverChoice(_) => ActionExpansionGroupKey {
            kind: ActionExpansionKind::DiscoverChoice,
            signature: format!("discover_choice/{}", pending_choice_label(engine)),
        },
        ClientInput::SubmitSelection(resolution) if resolution.scope == SelectionScope::Hand => {
            ActionExpansionGroupKey {
                kind: ActionExpansionKind::HandSelect,
                signature: format!(
                    "hand_select/{}/selected:{}",
                    pending_choice_label(engine),
                    resolution.selected.len()
                ),
            }
        }
        ClientInput::SubmitSelection(resolution) if resolution.scope == SelectionScope::Grid => {
            ActionExpansionGroupKey {
                kind: ActionExpansionKind::GridSelect,
                signature: format!(
                    "grid_select/{}/selected:{}",
                    pending_choice_label(engine),
                    resolution.selected.len()
                ),
            }
        }
        ClientInput::SubmitScryDiscard(indices) => ActionExpansionGroupKey {
            kind: ActionExpansionKind::ScryDiscard,
            signature: format!("scry_discard/selected:{}", indices.len()),
        },
        ClientInput::Cancel => ActionExpansionGroupKey {
            kind: ActionExpansionKind::Cancel,
            signature: format!("cancel/{}", pending_choice_label(engine)),
        },
        ClientInput::Proceed => ActionExpansionGroupKey {
            kind: ActionExpansionKind::Proceed,
            signature: format!("proceed/{}", engine_state_label(engine)),
        },
        _ => ActionExpansionGroupKey {
            kind: ActionExpansionKind::Other,
            signature: format!("{input:?}"),
        },
    }
}

fn pending_choice_label(engine: &EngineState) -> &'static str {
    match engine {
        EngineState::PendingChoice(choice) => match choice {
            crate::state::core::PendingChoice::HandSelect { .. } => "pending_hand_select",
            crate::state::core::PendingChoice::GridSelect { .. } => "pending_grid_select",
            crate::state::core::PendingChoice::DiscoverySelect(_) => "pending_discovery_select",
            crate::state::core::PendingChoice::CardRewardSelect { .. } => {
                "pending_card_reward_select"
            }
            crate::state::core::PendingChoice::ForeignInfluenceSelect { .. } => {
                "pending_foreign_influence_select"
            }
            crate::state::core::PendingChoice::ChooseOneSelect { .. } => {
                "pending_choose_one_select"
            }
            crate::state::core::PendingChoice::StanceChoice => "pending_stance_choice",
            _ => "pending_other_choice",
        },
        _ => "not_pending_choice",
    }
}

fn engine_state_label(engine: &EngineState) -> &'static str {
    match engine {
        EngineState::CombatPlayerTurn => "combat_player_turn",
        EngineState::CombatProcessing => "combat_processing",
        EngineState::CombatStart(_) => "combat_start",
        EngineState::PendingChoice(_) => "pending_choice",
        EngineState::RewardScreen(_) => "reward_screen",
        EngineState::RewardOverlay { .. } => "reward_overlay",
        EngineState::TreasureRoom(_) => "treasure_room",
        EngineState::Campfire => "campfire",
        EngineState::Shop(_) => "shop",
        EngineState::MapNavigation => "map_navigation",
        EngineState::MapOverlay { .. } => "map_overlay",
        EngineState::EventRoom => "event_room",
        EngineState::RunPendingChoice(_) => "run_pending_choice",
        EngineState::BossRelicSelect(_) => "boss_relic_select",
        EngineState::GameOver(_) => "game_over",
        EngineState::DoubleBossComplete => "double_boss_complete",
        EngineState::Act3VictoryComplete => "act3_victory_complete",
        EngineState::TrueVictoryComplete => "true_victory_complete",
    }
}
