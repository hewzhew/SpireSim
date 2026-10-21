use sts_oracle_runtime::ai::combat_search_v2::{CombatSearchV2ActionPreview, SearchTerminalLabel};
use sts_oracle_runtime::content::cards::CardId;
use sts_oracle_runtime::runtime::combat::CombatCard;
use sts_oracle_runtime::sim::combat::CombatPosition;
use sts_oracle_runtime::state::core::{ClientInput, EngineState};
use sts_oracle_runtime::test_support::{blank_test_combat, test_monster};

use super::super::focus::CombatReviewFocus;
use super::super::search_types::SearchDiagnosticProgressFacts;
use super::key_card_lifecycle;
use super::types::CardZoneLabel;
use super::types::KeyCardReason;

fn focus_with_action(action_key: String, input: ClientInput) -> CombatReviewFocus {
    let full_action = CombatSearchV2ActionPreview {
        action_key: action_key.clone(),
        input: input.clone(),
    };
    CombatReviewFocus {
        selected_review: "test_review",
        reason: "test",
        progress: SearchDiagnosticProgressFacts {
            source: "best_complete",
            terminal: SearchTerminalLabel::Loss,
            estimated: false,
            final_hp: 0,
            hp_loss: 80,
            turns: 1,
            potions_used: 0,
            cards_played: 1,
            living_enemy_count: 1,
            total_enemy_hp: 10,
            half_dead_enemy_count: 0,
            awakened_one_phase: None,
            awakened_one_phase_observation: "fixture",
            visible_incoming_damage: None,
            action_count: Some(1),
            exact_prefix_action_count: Some(1),
            action_key_preview: vec![action_key],
            input_preview: vec![input],
            full_action_preview: vec![full_action],
        },
    }
}

#[test]
fn records_played_triggered_scaling_card_from_focus_replay() {
    let mut combat = blank_test_combat();
    let demon_form = CombatCard::for_scenario(CardId::DemonForm, 42);
    let action_key = format!(
        "combat/play_card/hand:0/card:Demon Form+0#{}/target:none",
        demon_form.lineage
    );
    combat.meta.master_deck_snapshot = vec![demon_form.clone()].into();
    combat.zones.hand = vec![demon_form];
    combat.entities.monsters = vec![test_monster(
        sts_oracle_runtime::content::monsters::EnemyId::Cultist,
    )];
    let input = ClientInput::PlayCard {
        card_index: 0,
        target: None,
    };
    let focus = focus_with_action(action_key, input);
    let report = key_card_lifecycle(
        &CombatPosition::new(EngineState::CombatPlayerTurn, combat),
        Some(&focus),
    )
    .expect("Demon Form should be tracked");

    assert_eq!(report.tracked_cards.len(), 1);
    let lifecycle = &report.tracked_cards[0];
    assert_eq!(lifecycle.card, "Demon Form+0");
    assert_eq!(lifecycle.reason, KeyCardReason::StrengthScaling);
    assert_eq!(lifecycle.initial_zone, CardZoneLabel::Hand);
    assert!(lifecycle.played_in_replay);
    assert_eq!(
        lifecycle.first_play.as_ref().map(|play| play.step_index),
        Some(1)
    );
}
