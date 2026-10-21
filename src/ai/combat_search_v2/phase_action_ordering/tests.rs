use super::super::phase_profile::combat_search_phase_profile;
use super::*;
use crate::ai::combat_search_v2::enemy_phase_transition::EnemyPhaseTransitionHint;
use crate::content::cards::CardType;
use crate::content::monsters::EnemyId;
use crate::runtime::combat::CombatCard;
use crate::state::core::{ClientInput, EngineState};
use crate::test_support::{blank_test_combat, test_monster};

#[test]
fn lagavulin_sleep_phase_penalizes_wake_damage_and_rewards_power_setup() {
    let mut combat = blank_test_combat();
    let mut lagavulin = test_monster(EnemyId::Lagavulin);
    lagavulin.id = 1;
    lagavulin.lagavulin.is_out = false;
    combat.entities.monsters = vec![lagavulin];
    combat.zones.hand = vec![
        CombatCard::for_scenario(crate::content::cards::CardId::Strike, 10),
        CombatCard::for_scenario(crate::content::cards::CardId::Inflame, 11),
    ];
    let profile = combat_search_phase_profile(&EngineState::CombatPlayerTurn, &combat);

    let wake_hint = phase_action_ordering_hint(
        profile,
        super::super::CombatSearchPhaseGuardPluginId::Default,
        PhaseActionOrderingFacts {
            card_type: CardType::Attack,
            block: 0,
            mitigation: 0,
            target_progress: 6,
            target_lethal: false,
            future_debuff: false,
            access: PhaseActionAccessFacts::default(),
            target_enemy_id: Some(EnemyId::Lagavulin),
            target_has_stasis_card: false,
            phase_transition:
                super::super::enemy_phase_transition::enemy_phase_transition_hint_for_input(
                    &combat,
                    &ClientInput::PlayCard {
                        card_index: 0,
                        target: Some(1),
                    },
                    super::super::CombatSearchPhaseGuardPluginId::Default,
                ),
        },
    );
    let setup_hint = phase_action_ordering_hint(
        profile,
        super::super::CombatSearchPhaseGuardPluginId::Default,
        PhaseActionOrderingFacts {
            card_type: CardType::Power,
            block: 0,
            mitigation: 0,
            target_progress: 0,
            target_lethal: false,
            future_debuff: false,
            access: PhaseActionAccessFacts::default(),
            target_enemy_id: None,
            target_has_stasis_card: false,
            phase_transition:
                super::super::enemy_phase_transition::enemy_phase_transition_hint_for_input(
                    &combat,
                    &ClientInput::PlayCard {
                        card_index: 1,
                        target: None,
                    },
                    super::super::CombatSearchPhaseGuardPluginId::Default,
                ),
        },
    );

    assert!(wake_hint.role_rank_adjustment < 0);
    assert!(wake_hint.phase_transition_safety < 0);
    assert!(setup_hint.role_rank_adjustment > 0);
    assert!(setup_hint.phase_setup > wake_hint.phase_setup);
}

#[test]
fn guardian_defensive_phase_records_survival_tiebreak() {
    let mut combat = blank_test_combat();
    let mut guardian = test_monster(EnemyId::TheGuardian);
    guardian.guardian.is_open = false;
    combat.entities.monsters = vec![guardian];
    let profile = combat_search_phase_profile(&EngineState::CombatPlayerTurn, &combat);

    let hint = phase_action_ordering_hint(
        profile,
        super::super::CombatSearchPhaseGuardPluginId::Default,
        PhaseActionOrderingFacts {
            card_type: CardType::Skill,
            block: 8,
            mitigation: 3,
            target_progress: 0,
            target_lethal: false,
            future_debuff: false,
            access: PhaseActionAccessFacts::default(),
            target_enemy_id: None,
            target_has_stasis_card: false,
            phase_transition: EnemyPhaseTransitionHint::default(),
        },
    );

    assert_eq!(hint.phase_survival, 11);
    assert!(hint.has_signal());
}

#[test]
fn bronze_orb_stasis_phase_rewards_attacking_stasis_orb() {
    let mut combat = blank_test_combat();
    let mut automaton = test_monster(EnemyId::BronzeAutomaton);
    automaton.id = 1;
    let mut orb = test_monster(EnemyId::BronzeOrb);
    orb.id = 2;
    orb.set_planned_move_id(3);
    combat.entities.monsters = vec![automaton, orb];
    let profile = combat_search_phase_profile(&EngineState::CombatPlayerTurn, &combat);

    let orb_hint = phase_action_ordering_hint(
        profile,
        super::super::CombatSearchPhaseGuardPluginId::Default,
        PhaseActionOrderingFacts {
            card_type: CardType::Attack,
            block: 0,
            mitigation: 0,
            target_progress: 9,
            target_lethal: false,
            future_debuff: false,
            access: PhaseActionAccessFacts::default(),
            target_enemy_id: Some(EnemyId::BronzeOrb),
            target_has_stasis_card: false,
            phase_transition: EnemyPhaseTransitionHint::default(),
        },
    );
    let boss_hint = phase_action_ordering_hint(
        profile,
        super::super::CombatSearchPhaseGuardPluginId::Default,
        PhaseActionOrderingFacts {
            card_type: CardType::Attack,
            block: 0,
            mitigation: 0,
            target_progress: 9,
            target_lethal: false,
            future_debuff: false,
            access: PhaseActionAccessFacts::default(),
            target_enemy_id: Some(EnemyId::BronzeAutomaton),
            target_has_stasis_card: false,
            phase_transition: EnemyPhaseTransitionHint::default(),
        },
    );

    assert!(orb_hint.phase_setup > boss_hint.phase_setup);
}

#[test]
fn time_eater_clock_hint_penalizes_declared_and_conditional_access_at_warp() {
    let mut combat = blank_test_combat();
    let mut eater = test_monster(EnemyId::TimeEater);
    eater.id = 1;
    combat.entities.monsters = vec![eater];
    combat.entities.power_db.insert(
        1,
        vec![crate::runtime::combat::Power {
            power_type: crate::content::powers::PowerId::TimeWarp,
            instance_id: None,
            amount: 11,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let profile = combat_search_phase_profile(&EngineState::CombatPlayerTurn, &combat);

    let declared = phase_action_ordering_hint(
        profile,
        super::super::CombatSearchPhaseGuardPluginId::TimeEaterClockHint,
        PhaseActionOrderingFacts {
            card_type: CardType::Skill,
            block: 0,
            mitigation: 0,
            target_progress: 0,
            target_lethal: false,
            future_debuff: false,
            access: PhaseActionAccessFacts {
                declared_draw_cards: 1,
                ..PhaseActionAccessFacts::default()
            },
            target_enemy_id: None,
            target_has_stasis_card: false,
            phase_transition: EnemyPhaseTransitionHint::default(),
        },
    );
    let conditional = phase_action_ordering_hint(
        profile,
        super::super::CombatSearchPhaseGuardPluginId::TimeEaterClockHint,
        PhaseActionOrderingFacts {
            card_type: CardType::Skill,
            block: 0,
            mitigation: 0,
            target_progress: 0,
            target_lethal: false,
            future_debuff: false,
            access: PhaseActionAccessFacts {
                conditional_draw_cards: 1,
                ..PhaseActionAccessFacts::default()
            },
            target_enemy_id: None,
            target_has_stasis_card: false,
            phase_transition: EnemyPhaseTransitionHint::default(),
        },
    );

    assert!(declared.phase_transition_safety < 0);
    assert_eq!(declared, conditional);
}

#[test]
fn time_eater_clock_hint_penalizes_bad_access_more_than_clean_spam_at_warp() {
    let mut combat = blank_test_combat();
    let mut eater = test_monster(EnemyId::TimeEater);
    eater.id = 1;
    combat.entities.monsters = vec![eater];
    combat.entities.power_db.insert(
        1,
        vec![crate::runtime::combat::Power {
            power_type: crate::content::powers::PowerId::TimeWarp,
            instance_id: None,
            amount: 11,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let profile = combat_search_phase_profile(&EngineState::CombatPlayerTurn, &combat);
    let clean = phase_action_ordering_hint(
        profile,
        super::super::CombatSearchPhaseGuardPluginId::TimeEaterClockHint,
        PhaseActionOrderingFacts {
            card_type: CardType::Skill,
            block: 0,
            mitigation: 0,
            target_progress: 0,
            target_lethal: false,
            future_debuff: false,
            access: PhaseActionAccessFacts::default(),
            target_enemy_id: None,
            target_has_stasis_card: false,
            phase_transition: EnemyPhaseTransitionHint::default(),
        },
    );
    let risky = phase_action_ordering_hint(
        profile,
        super::super::CombatSearchPhaseGuardPluginId::TimeEaterClockHint,
        PhaseActionOrderingFacts {
            card_type: CardType::Skill,
            block: 0,
            mitigation: 0,
            target_progress: 0,
            target_lethal: false,
            future_debuff: false,
            access: PhaseActionAccessFacts {
                bad_draw_cards: 2,
                forced_turn_end: true,
                ..PhaseActionAccessFacts::default()
            },
            target_enemy_id: None,
            target_has_stasis_card: false,
            phase_transition: EnemyPhaseTransitionHint::default(),
        },
    );

    assert!(risky.phase_transition_safety < clean.phase_transition_safety);
}
