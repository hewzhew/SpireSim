use super::*;
use crate::content::monsters::EnemyId;
use crate::runtime::action::ActionInfo;
use std::collections::VecDeque;

#[test]
fn settled_draw_entry_resets_turn_cost_but_preserves_combat_cost_and_free_play() {
    let placements: [fn(&mut CombatState, CombatCard); 3] = [
        CombatState::add_card_to_draw_pile_top,
        CombatState::add_card_to_draw_pile_bottom,
        CombatState::add_card_to_draw_pile_random_spot,
    ];
    for place in placements {
        let mut state = crate::test_support::blank_test_combat();
        let mut card = CombatCard::for_scenario(CardId::Shockwave, 1);
        card.cost_modifier = 1;
        card.cost_for_turn = Some(0);
        card.free_to_play_once = true;
        card.retain_override = Some(true);
        card.base_damage_mut = 9;
        card.base_block_mut = 8;
        card.base_magic_num_mut = 7;
        let lineage = card.lineage;
        place(&mut state, card);
        let moved = &state.zones.draw_pile[0];
        assert_eq!(moved.lineage, lineage);
        assert_eq!(moved.cost_modifier, 1);
        assert_eq!(moved.cost_for_turn, None);
        assert_eq!(moved.combat_cost_without_turn_override_java(), 3);
        assert!(moved.free_to_play_once);
        assert_eq!(moved.retain_override, Some(true));
        assert_eq!(
            (
                moved.base_damage_mut,
                moved.base_block_mut,
                moved.base_magic_num_mut
            ),
            (0, 0, 0)
        );
    }
}

#[test]
fn combat_clone_shares_master_deck_until_a_branch_mutates_it() {
    let mut original = crate::test_support::blank_test_combat();
    original.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Strike, 7)].into();

    let mut branch = original.clone();
    assert!(original
        .meta
        .master_deck_snapshot
        .shares_storage_with(&branch.meta.master_deck_snapshot));

    branch
        .meta
        .master_deck_snapshot
        .with_cards_mut(|cards| cards[0].upgrades = 1);
    assert!(!original
        .meta
        .master_deck_snapshot
        .shares_storage_with(&branch.meta.master_deck_snapshot));
    assert_eq!(original.meta.master_deck_snapshot[0].upgrades, 0);
    assert_eq!(branch.meta.master_deck_snapshot[0].upgrades, 1);
}

#[test]
fn shared_master_deck_keeps_the_existing_json_array_shape() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Bash, 9)].into();
    let exact_hash_before = crate::ai::combat_state_key::combat_exact_state_hash_v2(
        &crate::state::EngineState::CombatPlayerTurn,
        &combat,
    );

    let json = serde_json::to_value(&combat).expect("combat state should serialize");
    assert!(json["meta"]["master_deck_snapshot"].is_array());
    let restored: CombatState =
        serde_json::from_value(json).expect("combat state should deserialize");
    assert_eq!(restored, combat);
    assert_eq!(
        crate::ai::combat_state_key::combat_exact_state_hash_v2(
            &crate::state::EngineState::CombatPlayerTurn,
            &restored,
        ),
        exact_hash_before,
        "deserialization must rebuild the same cached deck identity"
    );
}

#[test]
fn card_zones_draw_pile_top_is_index_zero() {
    let mut zones = CardZones {
        draw_pile: vec![CombatCard::for_scenario(CardId::Strike, 1)].into(),
        hand: vec![],
        discard_pile: vec![].into(),
        exhaust_pile: vec![].into(),
        limbo: vec![],
        queued_cards: VecDeque::new(),
        combat_lineage_allocator: CombatGeneratedLineageAllocator::new(),
    };

    zones.add_to_draw_pile_top(CombatCard::for_scenario(CardId::Defend, 2));
    zones.add_to_draw_pile_bottom(CombatCard::for_scenario(CardId::Bash, 3));

    assert_eq!(
        zones.draw_top_card().map(|card| card.id),
        Some(CardId::Defend)
    );
    assert_eq!(
        zones.draw_top_card().map(|card| card.id),
        Some(CardId::Strike)
    );
    assert_eq!(
        zones.draw_top_card().map(|card| card.id),
        Some(CardId::Bash)
    );
}

#[test]
fn card_zones_random_spot_maps_java_bottom_index_to_rust_top_index() {
    let mut zones = CardZones {
        draw_pile: vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
            CombatCard::for_scenario(CardId::Bash, 3),
        ]
        .into(),
        hand: vec![],
        discard_pile: vec![].into(),
        exhaust_pile: vec![].into(),
        limbo: vec![],
        queued_cards: VecDeque::new(),
        combat_lineage_allocator: CombatGeneratedLineageAllocator::new(),
    };

    zones.add_to_draw_pile_random_spot_from_java_index(
        CombatCard::for_scenario(CardId::Wound, 4),
        0,
    );
    assert_eq!(zones.draw_pile[3].id, CardId::Wound);

    zones
        .add_to_draw_pile_random_spot_from_java_index(CombatCard::for_scenario(CardId::Burn, 5), 3);
    assert_eq!(zones.draw_pile[1].id, CardId::Burn);
    assert_eq!(zones.draw_pile[0].id, CardId::Strike);
}

#[test]
fn card_zones_discard_pile_preserves_java_card_group_order() {
    let mut zones = CardZones {
        draw_pile: vec![].into(),
        hand: vec![],
        discard_pile: vec![].into(),
        exhaust_pile: vec![].into(),
        limbo: vec![],
        queued_cards: VecDeque::new(),
        combat_lineage_allocator: CombatGeneratedLineageAllocator::new(),
    };

    zones.add_to_discard_pile_top(CombatCard::for_scenario(CardId::Strike, 1));
    zones.add_to_discard_pile_top(CombatCard::for_scenario(CardId::Defend, 2));

    assert_eq!(zones.discard_pile[0].id, CardId::Strike);
    assert_eq!(zones.discard_pile[1].id, CardId::Defend);
}

#[test]
fn card_zones_exhaust_pile_preserves_java_card_group_order() {
    let mut zones = CardZones {
        draw_pile: vec![].into(),
        hand: vec![],
        discard_pile: vec![].into(),
        exhaust_pile: vec![].into(),
        limbo: vec![],
        queued_cards: VecDeque::new(),
        combat_lineage_allocator: CombatGeneratedLineageAllocator::new(),
    };

    zones.add_to_exhaust_pile_top(CombatCard::for_scenario(CardId::Strike, 1));
    zones.add_to_exhaust_pile_top(CombatCard::for_scenario(CardId::Defend, 2));

    assert_eq!(zones.exhaust_pile[0].id, CardId::Strike);
    assert_eq!(zones.exhaust_pile[1].id, CardId::Defend);
}

#[test]
fn card_zones_lineage_helper_updates_java_battle_instances_only() {
    let mut zones = CardZones {
        draw_pile: vec![CombatCard::for_scenario(CardId::Rampage, 7)].into(),
        hand: vec![CombatCard::for_scenario(CardId::Rampage, 7)],
        discard_pile: vec![CombatCard::for_scenario(CardId::Strike, 8)].into(),
        exhaust_pile: vec![CombatCard::for_scenario(CardId::Rampage, 7)].into(),
        limbo: vec![CombatCard::for_scenario(CardId::Rampage, 7)],
        queued_cards: VecDeque::from([QueuedCardPlay {
            card: CombatCard::for_scenario(CardId::Rampage, 7),
            target: None,
            energy_on_use: 1,
            ignore_energy_total: false,
            autoplay: false,
            random_target: false,
            is_end_turn_autoplay: false,
            purge_on_use: false,
            source: QueuedCardSource::Normal,
        }]),
        combat_lineage_allocator: CombatGeneratedLineageAllocator::new(),
    };

    zones.for_each_java_battle_instance_mut_by_lineage(
        crate::runtime::combat::CardLineageId::constructed_scenario(7),
        |card| {
            card.misc_value += 2;
        },
    );

    assert_eq!(zones.hand[0].misc_value, 2);
    assert_eq!(zones.draw_pile[0].misc_value, 2);
    assert_eq!(zones.exhaust_pile[0].misc_value, 2);
    assert_eq!(zones.limbo[0].misc_value, 2);
    assert_eq!(zones.discard_pile[0].misc_value, 0);
    assert_eq!(
        zones.queued_cards[0].card.misc_value, 0,
        "queued cards are not Java CardGroup battle instances"
    );

    zones.for_each_queued_instance_mut_by_lineage(
        crate::runtime::combat::CardLineageId::constructed_scenario(7),
        |card| {
            card.misc_value += 3;
        },
    );
    assert_eq!(zones.queued_cards[0].card.misc_value, 3);
}

#[test]
fn combat_card_update_cost_preserves_java_cost_for_turn_difference() {
    let mut card = CombatCard::for_scenario(CardId::BloodForBlood, 1);
    card.set_cost_for_turn_java(1);

    card.update_cost_java(-1);

    assert_eq!(card.combat_cost_without_turn_override_java(), 3);
    assert_eq!(card.cost_for_turn_java(), 0);
    assert_eq!(card.get_cost(), 0);
}

#[test]
fn combat_card_modify_cost_for_combat_matches_java_zero_turn_branch() {
    let mut card = CombatCard::for_scenario(CardId::BloodForBlood, 1);
    card.set_cost_for_turn_java(0);

    card.modify_cost_for_combat_java(-1);

    assert_eq!(card.combat_cost_without_turn_override_java(), 3);
    assert_eq!(card.cost_for_turn_java(), 0);
}

#[test]
fn combat_card_can_set_combat_cost_without_erasing_turn_override() {
    let mut card = CombatCard::for_scenario(CardId::BloodForBlood, 1);
    card.set_cost_for_turn_java(0);

    card.set_combat_cost_preserving_turn_java(1);

    assert_eq!(card.combat_cost_without_turn_override_java(), 1);
    assert_eq!(card.cost_for_turn_java(), 0);
}

#[test]
fn java_initialize_deck_order_places_innate_on_rust_top_after_reversing() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.draw_pile = (vec![
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::Writhe, 2),
        CombatCard::for_scenario(CardId::Defend, 3),
        CombatCard::for_scenario(CardId::Pride, 4),
    ])
    .into();

    state.apply_java_initialize_deck_order_after_shuffle();

    assert_eq!(
        state
            .zones
            .draw_pile
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>(),
        vec![
            CardId::Pride,
            CardId::Writhe,
            CardId::Defend,
            CardId::Strike
        ]
    );
}

#[test]
fn engine_runtime_queue_actions_matches_java_add_to_top_order() {
    let mut engine = EngineRuntime {
        action_queue: VecDeque::new(),
    };

    engine.push_back(Action::DrawCards(99));
    engine.queue_actions(smallvec::smallvec![
        ActionInfo {
            action: Action::DrawCards(1),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::GainEnergy { amount: 2 },
            insertion_mode: AddTo::Top,
        },
        ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: 3,
            },
            insertion_mode: AddTo::Top,
        },
    ]);

    assert_eq!(
        engine.pop_front(),
        Some(Action::GainBlock {
            target: 0,
            amount: 3,
        }),
        "later addToTop calls should execute before earlier top calls"
    );
    assert_eq!(
        engine.pop_front(),
        Some(Action::GainEnergy { amount: 2 }),
        "earlier addToTop calls should remain ahead of existing queued actions"
    );
    assert_eq!(
        engine.pop_front(),
        Some(Action::DrawCards(99)),
        "existing queued actions remain ahead of later addToBot actions"
    );
    assert_eq!(
        engine.pop_front(),
        Some(Action::DrawCards(1)),
        "addToBot actions should append after existing queued actions"
    );
}

#[test]
fn turn_runtime_begin_next_player_turn_resets_turn_counters_and_energy() {
    let mut turn = TurnRuntime {
        turn_count: 2,
        current_phase: CombatPhase::MonsterTurn,
        energy: 1,
        turn_start_draw_modifier: -1,
        counters: EphemeralCounters {
            cards_played_this_turn: 4,
            attacks_played_this_turn: 2,
            cards_discarded_this_turn: 3,
            card_ids_played_this_turn: vec![CardId::Strike, CardId::Defend],
            card_ids_played_this_combat: vec![CardId::Zap],
            orbs_channeled_this_turn: vec![OrbId::Lightning],
            orbs_channeled_this_combat: vec![OrbId::Lightning, OrbId::Frost],
            mantra_gained_this_combat: 4,
            times_damaged_this_combat: 3,
            victory_triggered: false,
            discovery_cost_for_turn: None,
            early_end_turn_pending: true,
            skip_monster_turn_pending: true,
            end_turn_hand_discarded: true,
            end_turn_powers_triggered: true,
            player_escaping: false,
            escape_pending_reward: false,
        },
    };

    turn.begin_next_player_turn(3);

    assert_eq!(turn.turn_count, 3);
    assert_eq!(turn.current_phase, CombatPhase::PlayerTurn);
    assert_eq!(turn.energy, 3);
    assert_eq!(turn.counters.cards_played_this_turn, 0);
    assert_eq!(turn.counters.attacks_played_this_turn, 0);
    assert_eq!(turn.counters.cards_discarded_this_turn, 0);
    assert!(turn.counters.card_ids_played_this_turn.is_empty());
    assert_eq!(
        turn.counters.card_ids_played_this_combat,
        vec![CardId::Zap],
        "combat-wide played-card history should remain untouched"
    );
    assert!(turn.counters.orbs_channeled_this_turn.is_empty());
    assert!(!turn.counters.end_turn_hand_discarded);
    assert!(!turn.counters.end_turn_powers_triggered);
    assert_eq!(
        turn.counters.orbs_channeled_this_combat,
        vec![OrbId::Lightning, OrbId::Frost],
        "combat-wide orb channel history should remain untouched"
    );
    assert_eq!(
        turn.counters.mantra_gained_this_combat, 4,
        "combat-wide mantra history should remain untouched"
    );
    assert_eq!(
        turn.counters.times_damaged_this_combat, 3,
        "combat-wide counters should remain untouched"
    );
    assert!(
        turn.counters.early_end_turn_pending,
        "unrelated flags should not be reset by turn-start setup"
    );
    assert!(
        !turn.counters.skip_monster_turn_pending,
        "Java clears room.skipMonsterTurn when the next player turn starts"
    );
    assert_eq!(turn.turn_start_draw_modifier, -1);
}

#[test]
fn turn_runtime_counter_helpers_update_expected_flags() {
    let mut turn = TurnRuntime::fresh_player_turn(3);

    turn.mark_early_end_turn_pending();
    turn.mark_player_escaping();
    turn.mark_escape_pending_reward();
    turn.mark_victory_triggered();
    turn.increment_cards_played();
    turn.increment_attacks_played();
    turn.increment_times_damaged_this_combat();
    turn.set_discovery_cost_for_turn(Some(1));

    assert!(turn.counters.early_end_turn_pending);
    assert!(turn.counters.player_escaping);
    assert!(turn.counters.escape_pending_reward);
    assert!(turn.counters.victory_triggered);
    assert_eq!(turn.counters.cards_played_this_turn, 1);
    assert_eq!(turn.counters.attacks_played_this_turn, 1);
    assert_eq!(turn.counters.times_damaged_this_combat, 1);
    assert_eq!(turn.take_discovery_cost_for_turn(), Some(1));
    assert_eq!(turn.take_discovery_cost_for_turn(), None);
}

#[test]
fn turn_runtime_can_clear_transition_flags_without_touching_other_counters() {
    let mut turn = TurnRuntime::fresh_player_turn(3);
    turn.mark_early_end_turn_pending();
    turn.mark_escape_pending_reward();
    turn.increment_times_damaged_this_combat();

    turn.clear_early_end_turn_pending();
    turn.clear_escape_pending_reward();

    assert!(!turn.counters.early_end_turn_pending);
    assert!(!turn.counters.escape_pending_reward);
    assert_eq!(turn.counters.times_damaged_this_combat, 1);
}

#[test]
fn monster_turn_plan_and_preview_follow_planned_visible_spec() {
    let spec = MonsterMoveSpec::Attack(AttackSpec {
        base_damage: 6,
        hits: 2,
        damage_kind: DamageKind::Normal,
    });
    let monster = MonsterEntity {
        id: 1,
        enemy_id: EnemyId::JawWorm,
        current_hp: 30,
        max_hp: 30,
        block: 0,
        slot: 0,
        is_dying: false,
        is_escaped: false,
        half_dead: false,
        move_state: MonsterMoveState {
            planned_move_id: 7,
            history: VecDeque::from([1, 7]),
            planned_steps: Some(spec.to_steps()),
            planned_visible_spec: Some(spec.clone()),
        },
        logical_position: 0,
        runtime: Default::default(),
    };

    let plan = monster.turn_plan();
    let preview = crate::sim::combat_projection::project_monster_move_preview(&monster);

    assert_eq!(plan.move_id, 7);
    assert!(matches!(plan.summary_spec(), MonsterMoveSpec::Attack(_)));
    assert_eq!(preview.damage_per_hit, Some(6));
    assert_eq!(preview.hits, 2);
    assert_eq!(preview.total_damage, Some(12));
    assert!(crate::sim::combat_projection::monster_has_visible_attack(
        &monster
    ));
    assert_eq!(
        crate::sim::combat_projection::monster_preview_total_damage(&monster),
        12
    );
}

#[test]
fn monster_turn_plan_uses_planned_steps_damage_without_visible_spec() {
    let monster = MonsterEntity {
        id: 1,
        enemy_id: EnemyId::JawWorm,
        current_hp: 12,
        max_hp: 12,
        block: 0,
        slot: 0,
        is_dying: false,
        is_escaped: false,
        half_dead: false,
        move_state: MonsterMoveState {
            planned_move_id: 3,
            history: VecDeque::from([4, 3]),
            planned_steps: Some(
                MonsterMoveSpec::Attack(AttackSpec {
                    base_damage: 7,
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                })
                .to_steps(),
            ),
            planned_visible_spec: None,
        },
        logical_position: 0,
        runtime: Default::default(),
    };

    let plan = monster.turn_plan();

    assert_eq!(plan.move_id, 3);
    assert_eq!(plan.attack().map(|attack| attack.base_damage), Some(7));
    assert_eq!(
        crate::sim::combat_projection::project_monster_move_preview(&monster).damage_per_hit,
        Some(7)
    );
}

#[test]
fn monster_turn_plan_hides_visible_spec_for_half_dead_or_dying_monsters() {
    let monster = MonsterEntity {
        id: 1,
        enemy_id: EnemyId::JawWorm,
        current_hp: 0,
        max_hp: 56,
        block: 0,
        slot: 0,
        is_dying: false,
        is_escaped: false,
        half_dead: true,
        move_state: MonsterMoveState {
            planned_move_id: 4,
            history: VecDeque::from([3, 4]),
            planned_steps: Some(
                MonsterMoveSpec::Attack(AttackSpec {
                    base_damage: 13,
                    hits: 1,
                    damage_kind: DamageKind::Normal,
                })
                .to_steps(),
            ),
            planned_visible_spec: Some(MonsterMoveSpec::Attack(AttackSpec {
                base_damage: 13,
                hits: 1,
                damage_kind: DamageKind::Normal,
            })),
        },
        logical_position: 0,
        runtime: Default::default(),
    };

    let plan = monster.turn_plan();
    let preview = crate::sim::combat_projection::project_monster_move_preview(&monster);

    assert_eq!(plan.move_id, 4);
    assert!(matches!(plan.summary_spec(), MonsterMoveSpec::Unknown));
    assert_eq!(preview.damage_per_hit, None);
    assert!(!crate::sim::combat_projection::monster_has_visible_attack(
        &monster
    ));
    assert_eq!(
        crate::sim::combat_projection::monster_preview_total_damage(&monster),
        0
    );
}

#[test]
fn clearing_current_monster_intent_preserves_public_executed_history() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.set_monster_protocol_visible_intent(7, Intent::Attack { damage: 6, hits: 1 });
    combat.set_monster_protocol_preview_damage_per_hit(7, 6);
    combat.record_monster_protocol_executed_move(7, 2);

    combat.clear_monster_protocol_observation(7);

    assert_eq!(combat.monster_protocol_visible_intent(7), &Intent::Unknown);
    assert_eq!(combat.monster_protocol_preview_damage_per_hit(7), 0);
    assert_eq!(combat.monster_protocol_executed_move_history(7), &[2]);
}
