use super::*;
use crate::content::monsters::EnemyId;
use crate::runtime::combat::{Power, PowerPayload};
use crate::test_support::{blank_test_combat, test_monster};

#[test]
fn phase_profile_reports_split_phase_and_pressure() {
    let mut combat = blank_test_combat();
    let mut slime = test_monster(EnemyId::AcidSlimeL);
    slime.id = 7;
    slime.current_hp = 30;
    slime.max_hp = 65;
    slime.set_planned_move_id(3);
    combat.entities.monsters = vec![slime];
    combat.entities.power_db.insert(
        7,
        vec![Power {
            power_type: crate::content::powers::PowerId::Split,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let profile = combat_search_phase_profile(&EngineState::CombatPlayerTurn, &combat);

    assert_eq!(profile.enemy_phase.split_pending_count, 1);
    assert_eq!(profile.enemy_phase.split_debt_hp, 30);
    assert_eq!(profile.pressure.survival_margin, 80);
    assert_eq!(profile.special_enemy_phase_count(), 1);
}

#[test]
fn phase_profile_marks_large_scry_as_high_fanout_rollout_boundary() {
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
        cards: vec![crate::content::cards::CardId::Strike; 7],
        card_lineages: crate::test_support::scenario_lineages(0..7),
    });
    let combat = blank_test_combat();

    let profile = combat_search_phase_profile(&engine, &combat);

    assert_eq!(
        profile.pending_choice.kind,
        Some(PendingChoicePhaseKind::ScrySelect)
    );
    assert_eq!(profile.pending_choice.candidate_count, 7);
    assert_eq!(profile.pending_choice.estimated_action_fanout, 128);
    assert!(profile.pending_choice.high_fanout);
}

#[test]
fn phase_profile_keeps_small_discovery_rollout_eligible() {
    let engine = EngineState::PendingChoice(crate::state::core::PendingChoice::DiscoverySelect(
        crate::state::core::DiscoveryChoiceState {
            cards: vec![
                crate::content::cards::CardId::Strike,
                crate::content::cards::CardId::Defend,
                crate::content::cards::CardId::Bash,
            ],
            colorless: false,
            card_type: None,
            amount: 1,
            can_skip: true,
        },
    ));
    let combat = blank_test_combat();

    let profile = combat_search_phase_profile(&engine, &combat);

    assert_eq!(profile.pending_choice.estimated_action_fanout, 4);
    assert!(!profile.pending_choice.high_fanout);
}
