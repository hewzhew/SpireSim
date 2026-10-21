use crate::ai::combat_search_v2::{
    CombatSearchV2ActionTrace, CombatSearchV2Config, CombatSearchV2OutcomeOrderKeyReport,
    CombatSearchV2StateSummary, CombatSearchV2TrajectoryReport, SearchTerminalLabel,
};
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, MetaChange, MonsterEntity};
use crate::runtime::monster_move::{AttackSpec, DamageKind, MonsterMoveSpec};
use crate::sim::combat::{CombatPosition, CombatTerminal};
use crate::sim::combat_action::CombatActionChoice;
use crate::sim::combat_action_surface::CombatSelectionInputEncodingV2;
use crate::state::core::{
    ActiveCombat, ClientInput, CombatContext, EngineState, GridSelectReason, PendingChoice,
    PileType, RoomCombatContext,
};
use crate::state::map::node::RoomType;
use crate::state::{HandSelectFilter, HandSelectReason};
use crate::state::{SelectionResolution, SelectionScope};

use super::burden::{newly_gained_persistent_curses, PersistentCurseBurdenSnapshot};
use super::cutpoint::{
    cutpoint_identity, group_cutpoints, locate_candidate_cutpoint, LocatedBurdenCutpoint,
};
use super::outcomes::{probe_cutpoint_actions, probe_grouped_cutpoint};
use super::{
    aggregate_cutpoints, conclusion_from_aggregate, PersistentBurdenCutpointActionDomainV1,
    PersistentBurdenCutpointAggregateV1, PersistentBurdenCutpointConclusionV1,
    PersistentBurdenCutpointInputOutcomeKindV1,
};
use crate::eval::run_control::combat_line_outcome::newly_gained_curses;
use crate::eval::run_control::session::{RunControlConfig, RunControlSession};

fn session_with_combat(mut combat: crate::runtime::combat::CombatState) -> RunControlSession {
    let engine = EngineState::CombatPlayerTurn;
    combat.meta.player_class = "Ironclad".to_string();
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.run_state.current_hp = combat.entities.player.current_hp;
    session.run_state.max_hp = combat.entities.player.max_hp;
    session.run_state.gold = combat.entities.player.gold;
    session.run_state.master_deck = combat.meta.master_deck_snapshot.to_vec();
    session.run_state.relics = combat.entities.player.relics.clone();
    session.run_state.potions = combat.entities.potions.clone();
    session.run_state.rng_pool = combat.rng.pool.clone();
    session.engine_state = engine.clone();
    session.active_combat = Some(ActiveCombat::new(
        engine,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    session
}

fn planned_jaw_worm() -> MonsterEntity {
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    let attack = MonsterMoveSpec::Attack(AttackSpec {
        base_damage: 5,
        hits: 1,
        damage_kind: DamageKind::Normal,
    });
    monster.set_planned_move_id(0);
    monster.set_planned_steps(attack.to_steps());
    monster.set_planned_visible_spec(Some(attack));
    monster
}

fn trajectory(actions: Vec<CombatSearchV2ActionTrace>) -> CombatSearchV2TrajectoryReport {
    let shorter_line = -(actions.len() as i32);
    CombatSearchV2TrajectoryReport {
        terminal: SearchTerminalLabel::Win,
        estimated: false,
        outcome_order_key: CombatSearchV2OutcomeOrderKeyReport {
            terminal_rank: 2,
            run_hygiene: 0,
            persistent_adjusted_hp: 80,
            final_hp: 80,
            persistent_run_value: 0,
            potion_conservation: 0,
            faster_turns: -1,
            fewer_cards_played: 0,
            enemy_progress: 0,
            shorter_line,
        },
        actions,
        final_hp: 80,
        final_max_hp: 80,
        persistent_run_value: 0,
        final_block: 0,
        hp_loss: 0,
        turns: 1,
        potions_used: 0,
        potions_discarded: 0,
        cards_played: 0,
        enemy_final_state: Vec::new(),
        final_state: CombatSearchV2StateSummary {
            engine_state: "RewardScreen".to_string(),
            terminal: SearchTerminalLabel::Win,
            player_hp: 80,
            player_block: 0,
            energy: 0,
            turn_count: 1,
            living_enemy_count: 0,
            total_enemy_hp: 0,
            visible_incoming_damage: 0,
            enemy_slots: Vec::new(),
            hand_count: 0,
            draw_count: 0,
            discard_count: 0,
            exhaust_count: 0,
            limbo_count: 0,
            queued_cards_count: 0,
        },
    }
}

fn trace(step_index: usize, choice: CombatActionChoice) -> CombatSearchV2ActionTrace {
    CombatSearchV2ActionTrace {
        step_index,
        action_id: step_index,
        action_key: choice.action_key,
        action_debug: choice.action_debug,
        input: choice.input,
    }
}

fn fixture_line_with_neutral_then_curse_input() -> (
    RunControlSession,
    CombatSearchV2Config,
    CombatSearchV2TrajectoryReport,
) {
    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Strike, 7)].into();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 11),
        CombatCard::for_scenario(CardId::Strike, 12),
    ];
    let monster = planned_jaw_worm();
    let monster_id = monster.id;
    combat.entities.monsters = vec![monster];
    combat.queue_action_back(Action::AddCardToMasterDeck {
        card_id: CardId::Parasite,
        source: monster_id,
    });
    combat.queue_action_back(Action::SuspendForHandSelect {
        min: 1,
        max: 1,
        can_cancel: false,
        filter: HandSelectFilter::Any,
        reason: HandSelectReason::Exhaust,
    });
    combat.queue_action_back(Action::InstantKill { target: monster_id });

    let session = session_with_combat(combat);
    let first_input = ClientInput::EndTurn;
    let first_position = session
        .current_active_combat_position()
        .expect("combat position");
    let first_choice = CombatActionChoice::from_input(&first_position.combat, first_input.clone());

    let mut after_first = session.clone();
    after_first.apply_input(first_input).expect("first input");
    let second_position = after_first
        .current_active_combat_position()
        .expect("pending choice position");
    let second_input = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
        SelectionScope::Hand,
        [crate::test_support::scenario_lineage(11)],
    ));
    let second_choice =
        CombatActionChoice::from_input(&second_position.combat, second_input.clone());

    (
        session,
        CombatSearchV2Config::default(),
        trajectory(vec![trace(0, first_choice), trace(1, second_choice)]),
    )
}

fn fixture_cutpoint_session() -> (RunControlSession, CombatPosition) {
    let mut combat = crate::test_support::blank_test_combat();
    let mut card = CombatCard::for_scenario(CardId::RitualDagger, 21);
    card.misc_value = 15;
    combat.meta.master_deck_snapshot = vec![card].into();
    combat.entities.monsters = vec![planned_jaw_worm()];
    let session = session_with_combat(combat);
    let position = session
        .current_active_combat_position()
        .expect("combat position");
    (session, position)
}

#[test]
fn persistent_curse_burden_does_not_double_count_materialization() {
    let (mut pending, _) = fixture_cutpoint_session();
    pending
        .active_combat
        .as_mut()
        .expect("active combat")
        .combat_state
        .meta
        .meta_changes
        .push(MetaChange::AddCardToMasterDeck(
            crate::test_support::test_master_deck_acquisition(CardId::Parasite, 999),
        ));
    pending
        .run_state
        .master_deck
        .push(CombatCard::for_scenario(CardId::Parasite, 999));
    let pending_snapshot = PersistentCurseBurdenSnapshot::capture(&pending);

    let mut materialized = pending.clone();
    materialized.active_combat = None;

    let materialized_snapshot = PersistentCurseBurdenSnapshot::capture(&materialized);

    assert!(newly_gained_persistent_curses(&pending_snapshot, &materialized_snapshot).is_empty());
}

fn fixture_located_cutpoint(retained_index: usize, label: &str) -> LocatedBurdenCutpoint {
    let (mut session, position) = fixture_cutpoint_session();
    if label == "different" {
        session.run_state.gold += 1;
    }
    let identity = cutpoint_identity(&session, &position);
    LocatedBurdenCutpoint {
        retained_index,
        trigger_step_index: 4,
        trigger_action_key: "combat/end_turn".to_string(),
        trigger_input: ClientInput::EndTurn,
        trigger_gained_curse_counts: Vec::new(),
        potions_used_before: 0,
        identity,
        session,
        position,
    }
}

fn fixture_writhing_mass_reactive_cutpoint() -> LocatedBurdenCutpoint {
    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Strike, 31)].into();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 31)];
    combat.update_hand_cards();

    let mut mass = crate::test_support::test_monster(EnemyId::WrithingMass);
    mass.current_hp = 50;
    mass.max_hp = 50;
    mass.writhing_mass.first_move = false;
    let attack = MonsterMoveSpec::Attack(AttackSpec {
        base_damage: 32,
        hits: 1,
        damage_kind: DamageKind::Normal,
    });
    mass.set_planned_move_id(0);
    mass.set_planned_steps(attack.to_steps());
    mass.set_planned_visible_spec(Some(attack));
    mass.move_history_mut().push_back(0);
    combat.entities.monsters = vec![mass];
    let mass_snapshot = combat.entities.monsters[0].clone();
    for action in crate::content::monsters::resolve_pre_battle_actions(
        &mut combat,
        EnemyId::WrithingMass,
        &mass_snapshot,
        crate::content::monsters::PreBattleLegacyRng::MonsterHp,
    ) {
        crate::engine::action_handlers::execute_action(action, &mut combat);
    }

    let session = session_with_combat(combat);
    let position = session
        .current_active_combat_position()
        .expect("combat position");
    LocatedBurdenCutpoint {
        retained_index: 0,
        trigger_step_index: 2,
        trigger_action_key: "combat/end_turn".to_string(),
        trigger_input: ClientInput::EndTurn,
        trigger_gained_curse_counts: Vec::new(),
        potions_used_before: 0,
        identity: cutpoint_identity(&session, &position),
        session,
        position,
    }
}

fn conclude_probe(
    clean_wins: usize,
    plan_changes: usize,
    input_failures: usize,
    replay_failures: usize,
) -> PersistentBurdenCutpointConclusionV1 {
    conclusion_from_aggregate(
        &PersistentBurdenCutpointAggregateV1 {
            clean_terminal_win_count: clean_wins,
            living_enemy_plan_change_count: plan_changes,
            input_failure_count: input_failures,
            ..PersistentBurdenCutpointAggregateV1::default()
        },
        replay_failures,
    )
}

#[test]
fn pending_curse_is_located_before_later_combat_completion() {
    let (session, config, trajectory) = fixture_line_with_neutral_then_curse_input();
    let located = locate_candidate_cutpoint(&session, &config, 0, &trajectory)
        .expect("replay")
        .expect("burden cutpoint");

    assert_eq!(located.trigger_step_index, 0);
    assert_eq!(located.trigger_action_key, trajectory.actions[0].action_key);
    assert_eq!(located.trigger_gained_curse_counts.len(), 1);
    assert_eq!(
        located.trigger_gained_curse_counts[0].card,
        CardId::Parasite
    );
    assert_eq!(located.trigger_gained_curse_counts[0].count, 1);
    assert_eq!(located.potions_used_before, 0);
    assert!(matches!(
        located.position.engine,
        EngineState::CombatPlayerTurn
    ));
    assert!(newly_gained_curses(
        &session.run_state.master_deck,
        &located.session.run_state.master_deck,
    )
    .is_empty());
    let mut triggered = located.session.clone();
    triggered
        .apply_input(located.trigger_input.clone())
        .expect("trigger");
    assert_eq!(
        newly_gained_curses(
            &located.session.run_state.master_deck,
            &triggered.run_state.master_deck
        )
        .len(),
        1
    );
    assert!(triggered
        .active_combat
        .as_ref()
        .expect("combat remains active")
        .combat_state
        .meta
        .meta_changes
        .iter()
        .all(|change| !matches!(change, MetaChange::AddCardToMasterDeck(_))));
}

#[test]
fn unresolved_pending_curse_action_is_classified_as_new_curse() {
    let (session, config, trajectory) = fixture_line_with_neutral_then_curse_input();
    let located = locate_candidate_cutpoint(&session, &config, 0, &trajectory)
        .expect("replay")
        .expect("burden cutpoint");
    let outcome = probe_cutpoint_actions(&located, &config)
        .outcomes
        .into_iter()
        .find(|outcome| outcome.input == trajectory.actions[0].input)
        .expect("trigger outcome");

    assert_eq!(outcome.terminal, CombatTerminal::Unresolved);
    assert_eq!(
        outcome.kind,
        PersistentBurdenCutpointInputOutcomeKindV1::NewCurse
    );
    assert_eq!(outcome.gained_curse_counts.len(), 1);
    assert_eq!(outcome.gained_curse_counts[0].card, CardId::Parasite);
    assert_eq!(outcome.gained_curse_counts[0].count, 1);
}

#[test]
fn cutpoint_identity_includes_persistent_context_not_only_combat_hash() {
    let (session, position) = fixture_cutpoint_session();
    let base = cutpoint_identity(&session, &position);

    let mut changed_gold = session.clone();
    changed_gold.run_state.gold += 1;
    assert_ne!(
        base.canonical,
        cutpoint_identity(&changed_gold, &position).canonical
    );

    let mut changed_growth = session.clone();
    changed_growth.run_state.master_deck[0].misc_value += 1;
    assert_ne!(
        base.canonical,
        cutpoint_identity(&changed_growth, &position).canonical
    );

    let mut changed_rng = session.clone();
    changed_rng.run_state.rng_pool.card_rng.counter += 1;
    assert_ne!(
        base.canonical,
        cutpoint_identity(&changed_rng, &position).canonical
    );

    let mut changed_plan = position.clone();
    let next = changed_plan.combat.entities.monsters[0]
        .planned_move_id()
        .wrapping_add(1);
    changed_plan.combat.entities.monsters[0].set_planned_move_id(next);
    assert_ne!(
        base.canonical,
        cutpoint_identity(&session, &changed_plan).canonical
    );
}

#[test]
fn equivalent_cutpoints_group_and_keep_first_report_order() {
    let first = fixture_located_cutpoint(3, "same");
    let equivalent = fixture_located_cutpoint(8, "same");
    let distinct = fixture_located_cutpoint(9, "different");
    let grouped = group_cutpoints(vec![first, equivalent, distinct]);

    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].candidate_frequency, 2);
    assert_eq!(grouped[0].retained_indices, vec![3, 8]);
    assert_eq!(grouped[1].retained_indices, vec![9]);
}

#[test]
fn writhing_mass_reactive_attack_is_a_clean_plan_change() {
    let cutpoint = fixture_writhing_mass_reactive_cutpoint();
    let action_probe = probe_cutpoint_actions(&cutpoint, &CombatSearchV2Config::default());

    assert_eq!(
        action_probe.action_domain,
        PersistentBurdenCutpointActionDomainV1::AtomicActionsComplete
    );

    assert!(action_probe.outcomes.iter().any(|outcome| {
        outcome.kind == PersistentBurdenCutpointInputOutcomeKindV1::LivingEnemyPlanChanged
            && matches!(outcome.input, ClientInput::PlayCard { .. })
            && outcome.gained_curse_counts.is_empty()
    }));
}

#[test]
fn structured_pending_choices_fail_closed_and_are_explicit_in_report_schema() {
    let cases = [
        (
            PendingChoice::HandSelect {
                candidate_lineages: crate::test_support::scenario_lineages([31]),
                min_cards: 1,
                max_cards: 1,
                can_cancel: true,
                reason: HandSelectReason::Discard,
            },
            CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages,
        ),
        (
            PendingChoice::GridSelect {
                source_pile: PileType::Discard,
                candidate_lineages: crate::test_support::scenario_lineages([31]),
                min_cards: 1,
                max_cards: 1,
                can_cancel: true,
                reason: GridSelectReason::DiscardToHand,
            },
            CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages,
        ),
        (
            PendingChoice::ScrySelect {
                cards: vec![CardId::Strike],
                card_lineages: crate::test_support::scenario_lineages([31]),
            },
            CombatSelectionInputEncodingV2::SubmitScryDiscardIndices,
        ),
    ];
    let mut summaries = Vec::new();

    for (choice, expected_encoding) in cases {
        let mut cutpoint = fixture_located_cutpoint(0, "same");
        let card = CombatCard::for_scenario(CardId::Strike, 31);
        cutpoint.position.combat.zones.hand = vec![card.clone()];
        cutpoint.position.combat.zones.discard_pile = vec![card.clone()].into();
        cutpoint.position.combat.zones.draw_pile = (vec![card]).into();
        cutpoint.position.engine = EngineState::PendingChoice(choice);
        let grouped = group_cutpoints(vec![cutpoint])
            .pop()
            .expect("grouped cutpoint");
        let summary = probe_grouped_cutpoint(grouped, &CombatSearchV2Config::default());

        assert_eq!(
            summary.action_domain,
            PersistentBurdenCutpointActionDomainV1::UnsupportedStructuredActionDomain {
                selection_input_encodings: vec![expected_encoding],
            }
        );
        assert!(
            summary.outcomes.is_empty(),
            "atomic Cancel must not be probed"
        );
        let json = serde_json::to_value(&summary).expect("serialize cutpoint summary");
        assert_eq!(
            json["action_domain"]["status"],
            "unsupported_structured_action_domain"
        );
        summaries.push(summary);
    }

    let aggregate = aggregate_cutpoints(&summaries);
    assert_eq!(aggregate.unsupported_structured_action_domain_count, 3);
    assert_eq!(
        conclusion_from_aggregate(&aggregate, 0),
        PersistentBurdenCutpointConclusionV1::IncompleteDueToUnsupportedStructuredActionDomain
    );
}

#[test]
fn clean_win_then_plan_change_then_failures_define_conclusion_precedence() {
    assert_eq!(
        conclude_probe(1, 1, 1, 1),
        PersistentBurdenCutpointConclusionV1::CleanTerminalWinAvailable
    );
    assert_eq!(
        conclude_probe(0, 1, 1, 1),
        PersistentBurdenCutpointConclusionV1::BurdenTriggerPlanChangeAvailable
    );
    assert_eq!(
        conclude_probe(0, 0, 1, 1),
        PersistentBurdenCutpointConclusionV1::IncompleteDueToProbeFailures
    );
    assert_eq!(
        conclude_probe(0, 0, 0, 0),
        PersistentBurdenCutpointConclusionV1::NoOneActionEscapeObserved
    );
}

#[test]
fn input_failure_cannot_report_no_one_action_escape() {
    let aggregate = PersistentBurdenCutpointAggregateV1 {
        clean_terminal_win_count: 0,
        burden_trigger_count: 2,
        living_enemy_plan_change_count: 0,
        neutral_count: 4,
        input_failure_count: 1,
        unsupported_structured_action_domain_count: 0,
    };
    assert_eq!(
        conclusion_from_aggregate(&aggregate, 0),
        PersistentBurdenCutpointConclusionV1::IncompleteDueToProbeFailures
    );
}
