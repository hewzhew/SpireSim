use super::{
    apply_combat_meta_change, finish_active_combat, open_treasure_chest,
    remove_one_relic_from_rewards_after_chest_open, tick_run, tick_run_active,
    tick_run_active_with_observer,
};
use crate::content::cards::CardId;
use crate::content::relics::{RelicId, RelicState, RelicTier};
use crate::runtime::combat::CombatCard;
use crate::runtime::rng::{begin_sts_rng_trace, RngPool, StsRng, StsRngCall};
use crate::state::core::{
    ActiveCombat, ClientInput, CombatContext, EngineState, EventCombatContext, PostCombatReturn,
    RoomCombatContext,
};
use crate::state::map::node::{MapEdge, MapRoomNode, RoomType};
use crate::state::map::state::MapState;
use crate::state::rewards::{
    RewardItem, RewardScreenContext, RewardState, TreasureChestSize, TreasureChestState,
};
use crate::state::run::RunState;
use crate::state::selection::{
    DomainEvent, DomainEventSource, SelectionReason, SelectionResolution, SelectionScope,
    SelectionTargetRef,
};

fn run_state_with_first_room(room_type: RoomType) -> RunState {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    let mut first = MapRoomNode::new(0, 0);
    first.class = Some(room_type);
    first.edges.insert(MapEdge::new(0, 0, 0, 1));
    let mut second = MapRoomNode::new(0, 1);
    second.class = Some(RoomType::MonsterRoom);
    run_state.map = MapState::new(vec![vec![first], vec![second]]);
    run_state
}

#[test]
fn map_overlay_cancel_returns_to_stashed_reward_screen() {
    let mut run_state = run_state_with_first_room(RoomType::MonsterRoom);
    let reward = RewardState {
        items: vec![RewardItem::Gold { amount: 25 }],
        skippable: true,
        screen_context: RewardScreenContext::Standard,
        pending_card_choice: None,
        pending_card_reward_index: None,
    };
    let mut engine_state = EngineState::map_overlay(EngineState::RewardScreen(reward));
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::Cancel),
    ));

    let EngineState::RewardScreen(rewards) = engine_state else {
        panic!("cancel should reopen the reward screen");
    };
    assert_eq!(rewards.items, vec![RewardItem::Gold { amount: 25 }]);
    assert_eq!(run_state.map.current_y, -1);
    assert!(combat_state.is_none());
}

#[test]
fn map_overlay_path_selection_commits_travel_and_drops_return_screen() {
    let mut run_state = run_state_with_first_room(RoomType::MonsterRoom);
    let reward = RewardState {
        items: vec![RewardItem::Gold { amount: 25 }],
        skippable: true,
        screen_context: RewardScreenContext::Standard,
        pending_card_choice: None,
        pending_card_reward_index: None,
    };
    let mut engine_state = EngineState::map_overlay(EngineState::RewardScreen(reward));
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));

    assert_eq!(run_state.map.current_y, 0);
    assert_eq!(run_state.floor_num, 1);
    assert!(
        matches!(
            engine_state,
            EngineState::CombatStart(_) | EngineState::CombatPlayerTurn
        ),
        "selecting a map node should commit to the next room, got {engine_state:?}"
    );
}

#[test]
fn entering_a_room_reseeds_java_per_floor_rng_streams() {
    let seed = 91;
    let mut run_state = run_state_with_first_room(RoomType::RestRoom);
    run_state.seed = seed;
    run_state.rng_pool = RngPool::new(seed);
    let _ = run_state.rng_pool.monster_rng.random(99);
    let persistent_monster_rng = run_state.rng_pool.monster_rng.clone();
    let _ = run_state.rng_pool.monster_hp_rng.random(99);
    let _ = run_state.rng_pool.ai_rng.random(99);
    let _ = run_state.rng_pool.shuffle_rng.random(99);
    let _ = run_state.rng_pool.card_random_rng.random(99);
    let _ = run_state.rng_pool.misc_rng.random(99);

    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));

    assert_eq!(run_state.floor_num, 1);
    let floor_rng = StsRng::new(seed + 1);
    assert_eq!(run_state.rng_pool.monster_hp_rng, floor_rng);
    assert_eq!(run_state.rng_pool.ai_rng, floor_rng);
    assert_eq!(run_state.rng_pool.shuffle_rng, floor_rng);
    assert_eq!(run_state.rng_pool.card_random_rng, floor_rng);
    assert_eq!(run_state.rng_pool.misc_rng, floor_rng);
    assert_eq!(
        run_state.rng_pool.monster_rng, persistent_monster_rng,
        "persistent run RNG streams must not be reset at a floor transition"
    );
}

#[test]
fn finishing_combat_returns_java_rng_stream_state_to_the_run() {
    let mut run_state = RunState::new(91, 0, false, "Ironclad");
    let mut combat = crate::test_support::blank_test_combat();
    let _ = combat.rng.card_rng.random(99);
    let _ = combat.rng.card_random_rng.random(99);
    let expected = combat.rng.pool.clone();
    let mut engine_state = EngineState::MapNavigation;
    let mut active_combat = Some(ActiveCombat::new(
        EngineState::MapNavigation,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));

    assert!(finish_active_combat(
        &mut engine_state,
        &mut run_state,
        &mut active_combat,
    ));

    assert_eq!(run_state.rng_pool, expected);
    assert!(active_combat.is_none());
}

#[test]
fn map_boss_room_starts_boss_combat_and_uses_boss_reward_rules() {
    let mut run_state = run_state_with_first_room(RoomType::MonsterRoomBoss);
    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));

    assert!(matches!(engine_state, EngineState::CombatPlayerTurn));
    let combat = combat_state
        .as_mut()
        .expect("boss room should start combat");
    assert!(
        combat.meta.is_boss_fight,
        "boss-room combat must carry boss metadata into reward generation"
    );
    for monster in &mut combat.entities.monsters {
        monster.current_hp = 0;
        monster.is_dying = true;
    }
    engine_state = EngineState::CombatProcessing;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        None
    ));

    let EngineState::RewardScreen(rewards) = engine_state else {
        panic!("act 1 boss combat should open its reward screen, got {engine_state:?}");
    };
    let cards = rewards
        .items
        .iter()
        .find_map(|item| match item {
            RewardItem::Card { cards } => Some(cards),
            _ => None,
        })
        .expect("boss combat should append a card reward row");
    assert!(
        cards.iter().all(|card| {
            crate::content::cards::get_card_definition(card.id).rarity
                == crate::content::cards::CardRarity::Rare
        }),
        "boss combat reward cards must use Java's boss rare override: {cards:?}"
    );
    assert!(
        run_state.pending_boss_reward,
        "act 1/2 boss reward screen must be followed by boss relic selection"
    );
}

#[test]
fn map_combat_won_before_input_settles_directly_to_generated_rewards() {
    let mut run_state = run_state_with_first_room(RoomType::MonsterRoom);
    let mut lament = RelicState::new(RelicId::NeowsLament);
    lament.counter = 2;
    run_state.relics = vec![
        RelicState::new(RelicId::BurningBlood),
        lament,
        RelicState::new(RelicId::MercuryHourglass),
    ];
    let mut engine_state = EngineState::MapNavigation;
    let mut active_combat = None;

    let outcome = tick_run_active_with_observer(
        &mut engine_state,
        &mut run_state,
        &mut active_combat,
        Some(ClientInput::SelectMapNode(0)),
    );

    assert!(outcome.keep_running);
    assert!(
        outcome.finished_combat.is_some(),
        "a zero-action victory must remain observable to run-control accounting"
    );
    assert!(active_combat.is_none());
    let EngineState::RewardScreen(rewards) = engine_state else {
        panic!("pre-input victory should settle to combat rewards, got {engine_state:?}");
    };
    assert!(rewards
        .items
        .iter()
        .any(|item| matches!(item, RewardItem::Gold { .. })));
    assert_eq!(
        run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::NeowsLament)
            .map(|relic| relic.counter),
        Some(1),
        "combat-start relic state must be absorbed back into the run"
    );
}

#[test]
fn act3_a20_first_boss_waits_for_proceed_before_starting_second_boss() {
    use crate::content::monsters::factory::EncounterId;
    use crate::content::monsters::EnemyId;

    let mut run_state = RunState::new(1, 20, true, "Ironclad");
    run_state.act_num = 3;
    run_state.floor_num = 50;
    run_state.relics.push(RelicState::new(RelicId::MawBank));
    run_state.boss_list = vec![
        EncounterId::AwakenedOne,
        EncounterId::TimeEater,
        EncounterId::DonuAndDeca,
    ];
    run_state.boss_key = Some(EncounterId::AwakenedOne);
    assert_eq!(run_state.next_boss(), Some(EncounterId::AwakenedOne));

    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.is_boss_fight = true;
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::MawBank));
    let mut boss = crate::test_support::test_monster(EnemyId::AwakenedOne);
    boss.current_hp = 0;
    boss.is_dying = true;
    combat.entities.monsters.push(boss);

    let mut engine_state = EngineState::CombatProcessing;
    let mut combat_state = Some(combat);

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        None,
    ));

    assert_eq!(engine_state, EngineState::DoubleBossComplete);
    assert!(combat_state.is_none());
    assert_eq!(run_state.floor_num, 50);
    assert_eq!(run_state.boss_key, Some(EncounterId::AwakenedOne));
    assert_eq!(
        run_state.boss_list,
        vec![EncounterId::TimeEater, EncounterId::DonuAndDeca]
    );
    let settled_rng = run_state.rng_pool.clone();
    let settled_gold = run_state.gold;
    for input in [None, Some(ClientInput::Cancel)] {
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            input
        ));
        assert_eq!(engine_state, EngineState::DoubleBossComplete);
        assert_eq!(run_state.floor_num, 50);
        assert_eq!(run_state.rng_pool, settled_rng);
        assert_eq!(run_state.gold, settled_gold);
    }
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::Proceed)
    ));
    assert!(matches!(engine_state, EngineState::CombatPlayerTurn));
    assert!(combat_state.is_some());
    assert_eq!(run_state.floor_num, 51);
    assert_eq!((run_state.map.current_x, run_state.map.current_y), (0, 15));
    assert_eq!(run_state.rng_pool.shuffle_rng, StsRng::new(52));
    assert_eq!(run_state.boss_key, Some(EncounterId::TimeEater));
    assert_eq!(run_state.boss_list, vec![EncounterId::DonuAndDeca]);
    assert_eq!(run_state.gold, settled_gold + 12);
}

#[test]
fn late_boss_completion_rolls_unclaimed_gold_without_potion_or_card_rewards() {
    use crate::content::monsters::factory::EncounterId;
    for act in [3, 4] {
        for daily in [false, true] {
            let mut run = RunState::new(17, 20, true, "Ironclad");
            run.act_num = act;
            run.is_daily_run = daily;
            run.boss_list = vec![EncounterId::TimeEater, EncounterId::DonuAndDeca];
            let gold = run.gold;
            let potion_chance = run.potion_drop_chance_mod;
            let mut expected_rng = run.rng_pool.clone();
            if !daily {
                expected_rng.misc_rng.random_range(-5, 5);
            }
            let mut combat = crate::test_support::blank_test_combat();
            combat.meta.is_boss_fight = true;
            let mut engine = EngineState::RewardScreen(RewardState::new());
            super::finish_room_combat(&mut engine, &mut run, &mut combat);
            assert_eq!(run.rng_pool, expected_rng, "act {act}, daily {daily}");
            assert_eq!(run.gold, gold);
            assert_eq!(run.potion_drop_chance_mod, potion_chance);
            assert!(!matches!(engine, EngineState::RewardScreen(_)));
        }
    }
}

#[test]
fn act3_boss_with_all_keys_waits_for_proceed_then_enters_spire_heart() {
    use crate::content::monsters::factory::EncounterId;
    use crate::content::monsters::EnemyId;

    let mut run_state = RunState::new(1, 19, true, "Ironclad");
    run_state.act_num = 3;
    run_state.floor_num = 50;
    run_state.keys = [true, true, true];
    run_state.boss_list = vec![
        EncounterId::AwakenedOne,
        EncounterId::TimeEater,
        EncounterId::DonuAndDeca,
    ];
    run_state.boss_key = Some(EncounterId::AwakenedOne);
    assert_eq!(run_state.next_boss(), Some(EncounterId::AwakenedOne));

    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.is_boss_fight = true;
    let mut boss = crate::test_support::test_monster(EnemyId::AwakenedOne);
    boss.current_hp = 0;
    boss.is_dying = true;
    combat.entities.monsters.push(boss);

    let mut engine_state = EngineState::CombatProcessing;
    let mut combat_state = Some(combat);

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        None,
    ));

    assert_eq!(engine_state, EngineState::Act3VictoryComplete);
    assert_eq!(run_state.act_num, 3);
    assert_eq!(run_state.floor_num, 50);
    assert!(combat_state.is_none());
    let hp = run_state.current_hp;
    let rng = run_state.rng_pool.clone();
    for input in [None, Some(ClientInput::Cancel)] {
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            input
        ));
        assert_eq!(engine_state, EngineState::Act3VictoryComplete);
        assert_eq!(run_state.current_hp, hp);
        assert_eq!(run_state.floor_num, 50);
        assert_eq!(run_state.rng_pool, rng);
    }
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::Proceed)
    ));
    assert_eq!(engine_state, EngineState::EventRoom);
    assert_eq!(run_state.act_num, 3);
    assert_eq!(run_state.floor_num, 51);
    assert_eq!(run_state.rng_pool.shuffle_rng, StsRng::new(52));
    assert_eq!(run_state.current_hp, hp);
    assert_eq!(
        run_state.event_state.as_ref().unwrap().id,
        crate::state::events::EventId::SpireHeart
    );
    assert_eq!(run_state.event_state.as_ref().unwrap().current_screen, 0);
    assert!(combat_state.is_none());
}

#[test]
fn final_act_boss_victory_skips_normal_combat_rewards() {
    use crate::content::monsters::EnemyId;

    let mut run_state = run_state_with_first_room(RoomType::MonsterRoomBoss);
    run_state.act_num = 4;
    run_state.map = MapState::new(crate::state::map::generator::generate_ending_map());
    run_state.map.current_x = 3;
    run_state.map.current_y = 3;
    run_state.floor_num = 56;
    run_state.relics.push(RelicState::new(RelicId::MawBank));

    let mut combat = crate::test_support::blank_test_combat();
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::MawBank));
    combat.meta.is_boss_fight = true;
    let mut boss = crate::test_support::test_monster(EnemyId::CorruptHeart);
    boss.current_hp = 0;
    boss.is_dying = true;
    combat.entities.monsters.push(boss);

    let mut engine_state = EngineState::CombatProcessing;
    let mut combat_state = Some(combat);

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        None,
    ));
    assert_eq!(engine_state, EngineState::TrueVictoryComplete);
    assert!(combat_state.is_none());
    let gold_after_combat = run_state.gold;
    let rng_after_combat = serde_json::to_value(&run_state.rng_pool).unwrap();
    for input in [None, Some(ClientInput::Cancel)] {
        assert!(tick_run(
            &mut engine_state,
            &mut run_state,
            &mut combat_state,
            input
        ));
        assert_eq!(engine_state, EngineState::TrueVictoryComplete);
        assert_eq!(run_state.floor_num, 56);
        assert_eq!(run_state.gold, gold_after_combat);
        assert_eq!(
            serde_json::to_value(&run_state.rng_pool).unwrap(),
            rng_after_combat
        );
    }
    tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::Proceed),
    );
    assert_eq!(
        engine_state,
        EngineState::GameOver(crate::state::core::RunResult::Victory)
    );
    assert_eq!(run_state.floor_num, 57);
    assert_eq!((run_state.map.current_x, run_state.map.current_y), (3, 4));
    assert_eq!(run_state.gold, gold_after_combat + 12);
    assert_ne!(
        serde_json::to_value(&run_state.rng_pool).unwrap(),
        rng_after_combat
    );
}

#[test]
fn event_combat_rewards_do_not_call_standard_combat_loot_generator() {
    use crate::content::monsters::EnemyId;

    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state
        .relics
        .push(RelicState::new(RelicId::WhiteBeastStatue));
    let treasure_before = run_state.rng_pool.treasure_rng.counter;
    let relic_before = run_state.rng_pool.relic_rng.counter;
    let potion_before = run_state.rng_pool.potion_rng.counter;

    let mut event_rewards = RewardState::new();
    event_rewards.items.push(RewardItem::Gold { amount: 100 });
    let mut engine_state = EngineState::CombatProcessing;
    let event_context = EventCombatContext {
        rewards: event_rewards,
        reward_allowed: true,
        no_cards_in_rewards: false,
        elite_trigger: false,
        post_combat_return: PostCombatReturn::MapNavigation,
    };

    let mut combat = crate::test_support::blank_test_combat();
    combat.rng.pool = run_state.rng_pool.clone();
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::WhiteBeastStatue));
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.current_hp = 0;
    monster.is_dying = true;
    combat.entities.monsters.push(monster);
    let mut active_combat = Some(ActiveCombat::new(
        EngineState::CombatProcessing,
        combat,
        CombatContext::Event(event_context),
    ));

    assert!(tick_run_active(
        &mut engine_state,
        &mut run_state,
        &mut active_combat,
        Some(ClientInput::EndTurn),
    ));

    let EngineState::RewardScreen(rewards) = engine_state else {
        panic!("event combat should open a reward screen");
    };
    assert_eq!(
        run_state.rng_pool.treasure_rng.counter, treasure_before,
        "EventRoom combat does not add standard monster gold rewards"
    );
    assert_eq!(
        run_state.rng_pool.relic_rng.counter, relic_before,
        "EventRoom combat does not call MonsterRoomElite.dropReward or random relic reward generation"
    );
    assert!(
        run_state.rng_pool.potion_rng.counter > potion_before,
        "EventRoom addPotionToRewards still uses potionRng"
    );
    assert_eq!(run_state.potion_drop_chance_mod, -10);
    assert!(matches!(rewards.items[0], RewardItem::Gold { amount: 100 }));
    assert!(matches!(rewards.items[1], RewardItem::Potion { .. }));
    assert!(matches!(rewards.items[2], RewardItem::Card { .. }));
    assert_eq!(
        rewards
            .items
            .iter()
            .filter(|item| matches!(item, RewardItem::Gold { .. }))
            .count(),
        1,
        "event combat keeps pre-populated event gold without adding standard monster gold"
    );
}

#[test]
fn event_combat_no_cards_keeps_event_rewards_and_potion_but_skips_card_reward() {
    use crate::content::monsters::EnemyId;

    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state
        .relics
        .push(RelicState::new(RelicId::WhiteBeastStatue));

    let mut event_rewards = RewardState::new();
    event_rewards.items.push(RewardItem::Gold { amount: 100 });
    let mut engine_state = EngineState::CombatProcessing;
    let event_context = EventCombatContext {
        rewards: event_rewards,
        reward_allowed: true,
        no_cards_in_rewards: true,
        elite_trigger: false,
        post_combat_return: PostCombatReturn::MapNavigation,
    };

    let mut combat = crate::test_support::blank_test_combat();
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::WhiteBeastStatue));
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.current_hp = 0;
    monster.is_dying = true;
    combat.entities.monsters.push(monster);
    let mut active_combat = Some(ActiveCombat::new(
        EngineState::CombatProcessing,
        combat,
        CombatContext::Event(event_context),
    ));

    assert!(tick_run_active(
        &mut engine_state,
        &mut run_state,
        &mut active_combat,
        Some(ClientInput::EndTurn),
    ));

    let EngineState::RewardScreen(rewards) = engine_state else {
        panic!("event combat should open a reward screen");
    };
    assert!(
        rewards
            .items
            .iter()
            .any(|item| matches!(item, RewardItem::Gold { amount: 100 })),
        "no-card event combat should keep pre-populated event rewards"
    );
    assert!(
        rewards
            .items
            .iter()
            .any(|item| matches!(item, RewardItem::Potion { .. })),
        "no-card event combat should still add potion rewards"
    );
    assert!(
        rewards
            .items
            .iter()
            .all(|item| !matches!(item, RewardItem::Card { .. })),
        "no-card event combat should not generate card rewards"
    );
}

#[test]
fn relic_pending_choice_keeps_relic_source_while_event_state_is_present() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.event_state = Some(crate::state::events::EventState::new(
        crate::state::events::EventId::Neow,
    ));
    let _ = run_state.take_emitted_events();

    let Some(mut engine_state) =
        crate::content::relics::dollys_mirror::on_equip(&mut run_state, EngineState::EventRoom)
    else {
        panic!("Dolly's Mirror should request a deck duplicate target");
    };
    let mut combat_state = None;
    let duplicate_target_uuid = run_state.master_deck[0].lineage;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SubmitSelection(
            SelectionResolution::card_lineages(SelectionScope::Deck, [duplicate_target_uuid],)
        )),
    ));

    let events = run_state.take_emitted_events();
    assert!(
        events.iter().any(|event| matches!(
            event,
            DomainEvent::SelectionResolved {
                reason: SelectionReason::Duplicate,
                source: DomainEventSource::Relic(RelicId::DollysMirror),
                ..
            }
        )),
        "Dolly's Mirror target selection should be attributed to the relic, not the current event: {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            DomainEvent::CardObtained {
                source: DomainEventSource::Relic(RelicId::DollysMirror),
                ..
            }
        )),
        "Dolly's Mirror copied card should be attributed to the relic: {events:?}"
    );
}

#[test]
fn mandatory_run_pending_choice_cannot_be_cancelled() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    let original_deck_len = run_state.master_deck.len();
    let mut engine_state =
        EngineState::RunPendingChoice(crate::state::core::RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 1,
            reason: crate::state::core::RunPendingChoiceReason::Purge,
            source: DomainEventSource::Relic(RelicId::EmptyCage),
            return_state: Box::new(EngineState::MapNavigation),
        });
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::Cancel),
    ));

    assert!(
        matches!(engine_state, EngineState::RunPendingChoice(_)),
        "mandatory run pending choices should ignore cancel, got {engine_state:?}"
    );
    assert_eq!(run_state.master_deck.len(), original_deck_len);
}

#[test]
fn finished_combat_syncs_potion_slots_back_to_run_state() {
    use crate::content::monsters::EnemyId;
    use crate::content::potions::{Potion, PotionId};

    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.potions[0] = Some(Potion::new(PotionId::FruitJuice, 42));

    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.potions = run_state.potions.clone();
    combat.entities.potions[0] = None;
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.current_hp = 0;
    monster.is_dying = true;
    combat.entities.monsters.push(monster);

    let mut engine_state = EngineState::CombatProcessing;
    let mut active_combat = Some(ActiveCombat::new(
        EngineState::CombatProcessing,
        combat,
        CombatContext::Room(crate::state::core::RoomCombatContext {
            room_type: crate::state::map::node::RoomType::MonsterRoom,
        }),
    ));

    assert!(tick_run_active(
        &mut engine_state,
        &mut run_state,
        &mut active_combat,
        Some(ClientInput::EndTurn),
    ));
    assert!(
        run_state.potions[0].is_none(),
        "combat potion inventory must persist after combat ends"
    );
}

#[test]
fn smoke_marks_room_while_escape_is_still_in_active_combat() {
    let mut run = run_state_with_first_room(RoomType::MonsterRoom);
    let mut combat = crate::test_support::blank_test_combat();
    combat
        .entities
        .monsters
        .push(crate::test_support::test_monster(
            crate::content::monsters::EnemyId::JawWorm,
        ));
    combat.queue_action_back(crate::runtime::action::Action::FleeCombat);
    let mut state = EngineState::CombatProcessing;
    let mut active = Some(ActiveCombat::new(
        state.clone(),
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    assert!(tick_run_active(&mut state, &mut run, &mut active, None));
    assert!(
        active.is_some(),
        "escape remains active while the automatic animation settles"
    );
    assert!(
        run.room_smoked,
        "room flag is visible immediately after Smoke Bomb resolves"
    );
}

#[test]
fn smoke_automatically_completes_without_end_turn_damage_or_discard() {
    let mut run = run_state_with_first_room(RoomType::MonsterRoom);
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.player.current_hp = 30;
    combat.entities.player.max_hp = 80;
    combat.entities.player.relics.clear();
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::BurningBlood));
    combat.entities.player.facing_left = true;
    combat
        .entities
        .monsters
        .push(crate::test_support::test_monster(
            crate::content::monsters::EnemyId::JawWorm,
        ));
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Burn, 9001),
        CombatCard::for_scenario(CardId::AscendersBane, 9002),
    ];
    let hand = combat.zones.hand.clone();
    combat.queue_action_back(crate::runtime::action::Action::FleeCombat);
    let mut state = EngineState::CombatProcessing;
    let mut active = Some(ActiveCombat::new(
        state.clone(),
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    for _ in 0..16 {
        let outcome = tick_run_active_with_observer(&mut state, &mut run, &mut active, None);
        assert!(outcome.keep_running);
        if let Some(finished) = outcome.finished_combat {
            assert!(active.is_none());
            assert!(run.room_smoked);
            assert_eq!(
                run.current_hp, 36,
                "victory heal applies; Burn never triggers"
            );
            assert_eq!(finished.combat_state.zones.hand, hand);
            assert!(finished.combat_state.zones.exhaust_pile.is_empty());
            assert!(!finished.combat_state.entities.player.facing_left);
            assert!(matches!(state, EngineState::RewardScreen(ref r)
                if r.screen_context == RewardScreenContext::SmokedCombat && r.items.is_empty()));
            return;
        }
        assert_ne!(state, EngineState::CombatPlayerTurn);
    }
    panic!("escape did not settle automatically");
}

#[test]
fn smoked_combat_consumes_hidden_room_reward_rng_without_visible_rewards() {
    use crate::content::monsters::EnemyId;

    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state
        .relics
        .push(RelicState::new(RelicId::WhiteBeastStatue));
    let treasure_before = run_state.rng_pool.treasure_rng.counter;
    let potion_before = run_state.rng_pool.potion_rng.counter;
    let card_before = run_state.rng_pool.card_rng.counter;

    let mut combat = crate::test_support::blank_test_combat();
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::WhiteBeastStatue));
    combat.runtime.combat_smoked = true;
    combat
        .runtime
        .pending_rewards
        .push(RewardItem::StolenGold { amount: 40 });
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.current_hp = 0;
    monster.is_dying = true;
    combat.entities.monsters.push(monster);

    let mut engine_state = EngineState::CombatProcessing;
    let mut combat_state = Some(combat);

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        None,
    ));

    let EngineState::RewardScreen(rewards) = engine_state else {
        panic!("smoked combat should still reach a reward/proceed screen");
    };
    assert_eq!(rewards.screen_context, RewardScreenContext::SmokedCombat);
    assert!(
        rewards.items.is_empty(),
        "Java openCombat(smoked=true) does not call setupItemReward, so generated room rewards are not visible"
    );
    assert!(
        run_state.rng_pool.treasure_rng.counter > treasure_before,
        "Java still adds normal room gold before opening the smoked reward screen"
    );
    assert!(
        run_state.rng_pool.potion_rng.counter > potion_before,
        "Java still calls addPotionToRewards before opening the smoked reward screen"
    );
    assert_eq!(
        run_state.rng_pool.card_rng.counter, card_before,
        "Java smoked reward screen skips CombatRewardScreen.setupItemReward card generation"
    );
    assert_eq!(run_state.potion_drop_chance_mod, -10);
}

#[test]
fn mugged_all_escaped_normal_combat_skips_standard_gold_and_base_potion_chance() {
    use crate::content::monsters::EnemyId;

    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    let treasure_before = run_state.rng_pool.treasure_rng.counter;
    let potion_before = run_state.rng_pool.potion_rng.counter;

    let mut combat = crate::test_support::blank_test_combat();
    combat.runtime.combat_mugged = true;
    let mut monster = crate::test_support::test_monster(EnemyId::Looter);
    monster.is_escaped = true;
    combat.entities.monsters.push(monster);

    let mut engine_state = EngineState::CombatProcessing;
    let mut combat_state = Some(combat);

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        None,
    ));

    let EngineState::RewardScreen(rewards) = engine_state else {
        panic!("mugged escaped combat should still open a reward screen");
    };
    assert_eq!(rewards.screen_context, RewardScreenContext::MuggedCombat);
    assert_eq!(
        run_state.rng_pool.treasure_rng.counter, treasure_before,
        "Java skips ordinary MonsterRoom gold when every monster escaped"
    );
    assert_eq!(
        run_state.rng_pool.potion_rng.counter,
        potion_before + 1,
        "Java addPotionToRewards still rolls potionRng even when escaped monsters force chance to 0"
    );
    assert_eq!(
        run_state.potion_drop_chance_mod, 10,
        "the chance-0 potion roll follows the Java miss path"
    );
    assert!(
        !rewards
            .items
            .iter()
            .any(|item| matches!(item, RewardItem::Gold { .. } | RewardItem::Potion { .. })),
        "all-escaped ordinary MonsterRoom should not create standard gold or a base potion reward"
    );
    assert!(
        rewards
            .items
            .iter()
            .any(|item| matches!(item, RewardItem::Card { .. })),
        "CombatRewardScreen.setupItemReward still appends card rewards for mugged combat"
    );
}

#[test]
fn meal_ticket_shop_entry_heal_uses_relic_source_and_mark_of_bloom_guard() {
    let mut run_state = run_state_with_first_room(RoomType::ShopRoom);
    run_state.current_hp = 20;
    run_state.max_hp = 80;
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::MealTicket));
    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));
    assert_eq!(run_state.current_hp, 35);
    assert!(matches!(engine_state, EngineState::Shop(_)));

    let mut blocked = run_state_with_first_room(RoomType::ShopRoom);
    blocked.current_hp = 20;
    blocked.max_hp = 80;
    blocked.relics.clear();
    blocked.relics.push(RelicState::new(RelicId::MealTicket));
    blocked
        .relics
        .push(RelicState::new(RelicId::MarkOfTheBloom));
    let mut blocked_engine = EngineState::MapNavigation;
    let mut blocked_combat = None;

    assert!(tick_run(
        &mut blocked_engine,
        &mut blocked,
        &mut blocked_combat,
        Some(ClientInput::SelectMapNode(0)),
    ));
    assert_eq!(blocked.current_hp, 20);
    assert!(matches!(blocked_engine, EngineState::Shop(_)));
}

#[test]
fn treasure_room_uses_java_chest_reward_rolls_before_relic_pool_draw() {
    fn small_gold_common_chest_seed() -> u64 {
        (1..10_000)
            .find(|seed| {
                let mut rng = StsRng::new(*seed);
                rng.random_range(0, 99) < 50 && rng.random_range(0, 99) < 50
            })
            .expect("seed for small chest with gold and common relic")
    }

    let mut run_state = run_state_with_first_room(RoomType::TreasureRoom);
    run_state.relics.clear();
    run_state.rng_pool.treasure_rng = StsRng::new(small_gold_common_chest_seed());
    run_state.common_relic_pool = vec![RelicId::Anchor];
    run_state.uncommon_relic_pool = vec![RelicId::Sundial];
    run_state.rare_relic_pool = vec![RelicId::Mango];
    let relic_rng_before = run_state.rng_pool.relic_rng.counter;

    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));
    assert!(matches!(engine_state, EngineState::TreasureRoom(_)));
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::OpenChest),
    ));

    let EngineState::RewardScreen(rewards) = engine_state else {
        panic!("treasure room should open a reward screen");
    };
    assert!(
        matches!(rewards.items[0], RewardItem::Gold { .. }),
        "Java AbstractChest.open adds chest gold before the base chest relic"
    );
    assert_eq!(
        rewards
            .items
            .iter()
            .filter_map(|item| match item {
                RewardItem::Relic { relic_id } => Some(*relic_id),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![RelicId::Anchor],
        "Java chest reward tier is decided by treasureRng, then removes from that tier pool"
    );
    assert_eq!(
        run_state.rng_pool.relic_rng.counter, relic_rng_before,
        "Java chest tier selection does not consume relicRng"
    );
    assert_eq!(
        run_state.rng_pool.treasure_rng.counter, 3,
        "Java consumes treasureRng for chest size, chest reward roll, and non-daily gold jitter"
    );
}

#[test]
fn treasure_room_gold_reward_does_not_receive_golden_idol_bonus() {
    fn small_gold_common_chest_seed() -> u64 {
        (1..10_000)
            .find(|seed| {
                let mut rng = StsRng::new(*seed);
                rng.random_range(0, 99) < 50 && rng.random_range(0, 99) < 50
            })
            .expect("seed for small chest with gold and common relic")
    }

    let mut run_state = run_state_with_first_room(RoomType::TreasureRoom);
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::GoldenIdol));
    run_state.rng_pool.treasure_rng = StsRng::new(small_gold_common_chest_seed());
    run_state.common_relic_pool = vec![RelicId::Anchor];

    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));
    assert!(matches!(engine_state, EngineState::TreasureRoom(_)));
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::OpenChest),
    ));

    let EngineState::RewardScreen(mut rewards) = engine_state else {
        panic!("treasure room should open a reward screen");
    };
    assert_eq!(rewards.screen_context, RewardScreenContext::TreasureRoom);
    let RewardItem::Gold { amount } = rewards.items[0] else {
        panic!("small chest seed should create chest gold");
    };
    let gold_before = run_state.gold;

    crate::engine::reward_handler::handle(
        &mut run_state,
        &mut rewards,
        Some(ClientInput::ClaimReward(0)),
    );

    assert_eq!(
        run_state.gold,
        gold_before + amount,
        "Java RewardItem.applyGoldBonus skips Golden Idol inside TreasureRoom"
    );
}

#[test]
fn treasure_room_chest_can_be_skipped_after_entry_like_java_complete_room() {
    fn small_gold_common_chest_seed() -> u64 {
        (1..10_000)
            .find(|seed| {
                let mut rng = StsRng::new(*seed);
                rng.random_range(0, 99) < 50 && rng.random_range(0, 99) < 50
            })
            .expect("seed for small chest with gold and common relic")
    }

    let mut run_state = run_state_with_first_room(RoomType::TreasureRoom);
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::CursedKey));
    run_state.rng_pool.treasure_rng = StsRng::new(small_gold_common_chest_seed());
    run_state.common_relic_pool = vec![RelicId::Anchor];
    let deck_before = run_state.master_deck.len();
    let relic_pool_before = run_state.common_relic_pool.clone();

    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));
    assert!(matches!(engine_state, EngineState::TreasureRoom(_)));
    assert_eq!(
        run_state.rng_pool.treasure_rng.counter, 2,
        "Java TreasureRoom.onPlayerEntry constructs/randomizes the chest before opening"
    );

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::Proceed),
    ));

    assert!(matches!(engine_state, EngineState::MapNavigation));
    assert_eq!(
        run_state.master_deck.len(),
        deck_before,
        "Cursed Key only fires from AbstractChest.open(false), not from entering or skipping the room"
    );
    assert_eq!(
        run_state.common_relic_pool, relic_pool_before,
        "Skipping the chest must not consume the chest relic reward"
    );
    assert_eq!(
        run_state.rng_pool.treasure_rng.counter, 2,
        "Skipping avoids the non-daily chest gold jitter consumed inside AbstractChest.open"
    );
}

#[test]
fn cursed_key_chest_obtain_hooks_run_before_curse_obtained_event() {
    let mut run_state = run_state_with_first_room(RoomType::TreasureRoom);
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::CursedKey));
    run_state.relics.push(RelicState::new(RelicId::CeramicFish));
    run_state.common_relic_pool = vec![RelicId::Anchor];

    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));
    assert!(matches!(engine_state, EngineState::TreasureRoom(_)));
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::OpenChest),
    ));

    let events = run_state.take_emitted_events();
    let fish_gold_pos = events
        .iter()
        .position(|event| {
            matches!(
                event,
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Relic(RelicId::CursedKey),
                    ..
                }
            )
        })
        .expect("Cursed Key chest curse should run Ceramic Fish obtain hook");
    let obtained_pos = events
        .iter()
        .position(|event| {
            matches!(
                event,
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Relic(RelicId::CursedKey),
                } if crate::content::cards::get_curse_pool().contains(&card.id)
            )
        })
        .expect("Cursed Key chest opening should obtain a random curse");

    assert!(
        fish_gold_pos < obtained_pos,
        "Java CursedKey.onChestOpen queues ShowCardAndObtainEffect; that effect runs onObtainCard before Soul.obtain"
    );
}

#[test]
fn question_mark_tiny_chest_forces_actual_treasure_after_event_room_enter_hooks() {
    fn small_gold_common_chest_seed() -> u64 {
        (1..10_000)
            .find(|seed| {
                let mut rng = StsRng::new(*seed);
                rng.random_range(0, 99) < 50 && rng.random_range(0, 99) < 50
            })
            .expect("seed for small chest with gold and common relic")
    }

    let mut run_state = run_state_with_first_room(RoomType::EventRoom);
    run_state.relics.clear();
    let mut tiny_chest = RelicState::new(RelicId::TinyChest);
    tiny_chest.counter = 3;
    run_state.relics.push(tiny_chest);
    run_state
        .relics
        .push(RelicState::new(RelicId::SsserpentHead));
    run_state.rng_pool.treasure_rng = StsRng::new(small_gold_common_chest_seed());
    run_state.common_relic_pool = vec![RelicId::Anchor];
    let gold_before = run_state.gold;

    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));

    assert_eq!(
        run_state.gold,
        gold_before + 50,
        "Java SsserpentHead sees the original ? EventRoom during onEnterRoom, before EventHelper.roll replaces it"
    );
    assert_eq!(
        run_state.map.get_current_room_type(),
        Some(RoomType::TreasureRoom),
        "Java EventHelper.roll replaces the ? room with the actual rolled room"
    );
    let tiny_chest = run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::TinyChest)
        .expect("Tiny Chest should be present");
    assert_eq!(tiny_chest.counter, 0);
    assert_eq!(
        run_state.rng_pool.event_rng.counter, 1,
        "Java still consumes eventRng for EventHelper.roll before Tiny Chest forces the result"
    );
    assert!(
        run_state.event_state.is_none(),
        "forced treasure must not continue into specific event generation"
    );
    assert!(matches!(engine_state, EngineState::TreasureRoom(_)));
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::OpenChest),
    ));
    assert!(matches!(engine_state, EngineState::RewardScreen(_)));
}

#[test]
fn mysterious_sphere_room_entry_constructs_guardians_before_fight_confirmation() {
    use crate::state::events::EventId;
    let mut run = run_state_with_first_room(RoomType::EventRoom);
    run.ascension_level = 20;
    run.event_generator.monster_chance = 0.0;
    run.event_generator.shop_chance = 0.0;
    run.event_generator.treasure_chance = 0.0;
    run.event_generator.shrine_chance = 0.0;
    run.event_generator.event_pool = vec![EventId::MysteriousSphere];
    run.event_generator.shrine_pool.clear();
    run.event_generator.one_time_event_pool.clear();
    let mut engine = EngineState::MapNavigation;
    let mut combat = None;
    assert!(tick_run_active(
        &mut engine,
        &mut run,
        &mut combat,
        Some(ClientInput::SelectMapNode(0))
    ));
    assert!(matches!(engine, EngineState::EventRoom));
    let prepared = run
        .event_state
        .as_ref()
        .unwrap()
        .prepared_encounter
        .clone()
        .unwrap();
    assert_eq!(run.rng_pool.monster_hp_rng.counter, 4);
    assert_eq!(run.rng_pool.ai_rng.counter, 0);
    let hp_after_init = run.rng_pool.monster_hp_rng.clone();
    for _ in 0..2 {
        assert!(tick_run_active(
            &mut engine,
            &mut run,
            &mut combat,
            Some(ClientInput::EventChoice(0))
        ));
    }
    assert!(matches!(engine, EngineState::CombatPlayerTurn));
    let combat = combat.unwrap();
    assert_eq!(combat.combat_state.rng.monster_hp_rng, hp_after_init);
    assert_eq!(
        combat
            .combat_state
            .entities
            .monsters
            .iter()
            .map(|m| m.current_hp)
            .collect::<Vec<_>>(),
        prepared
            .monsters
            .iter()
            .map(|m| m.current_hp)
            .collect::<Vec<_>>()
    );
}

#[test]
fn event_room_specific_event_selection_uses_duplicate_event_rng_like_java() {
    use crate::state::events::EventId;

    let mut run_state = run_state_with_first_room(RoomType::EventRoom);
    run_state.event_generator.monster_chance = 0.0;
    run_state.event_generator.shop_chance = 0.0;
    run_state.event_generator.treasure_chance = 0.0;
    run_state.event_generator.shrine_chance = 0.0;
    run_state.event_generator.event_pool = vec![EventId::BigFish];
    run_state.event_generator.shrine_pool.clear();
    run_state.event_generator.one_time_event_pool.clear();
    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;
    let trace = begin_sts_rng_trace().unwrap();

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));
    let calls = trace
        .finish()
        .unwrap()
        .into_iter()
        .map(|event| event.call)
        .collect::<Vec<_>>();

    assert!(matches!(engine_state, EngineState::EventRoom));
    assert_eq!(
        run_state
            .event_state
            .as_ref()
            .expect("event state should be initialized")
            .id,
        EventId::BigFish
    );
    assert_eq!(
        run_state
            .event_state
            .as_ref()
            .expect("event state should be initialized")
            .internal_state,
        run_state.max_hp / 3,
        "Big Fish freezes its Banana amount when the event opens"
    );
    assert!(
        run_state.event_generator.event_pool.is_empty(),
        "Java generateEvent mutates the event pool even though it uses a duplicate RNG"
    );
    assert_eq!(
        run_state.rng_pool.event_rng.counter, 1,
        "Java commits only EventHelper.roll's eventRng consumption; EventRoom.onPlayerEntry selects the concrete event with a duplicate RNG"
    );
    assert!(matches!(
        calls.as_slice(),
        [
            StsRngCall::RandomFloat { .. },
            StsRngCall::RandomInt { range: 999, .. },
            StsRngCall::RandomFloatRange { range: 1.0, .. },
            StsRngCall::RandomInt { range: 0, .. },
        ]
    ));
}

#[test]
fn eternal_feather_rest_room_heal_uses_relic_source_and_mark_of_bloom_guard() {
    let mut run_state = run_state_with_first_room(RoomType::RestRoom);
    run_state.current_hp = 20;
    run_state.max_hp = 80;
    run_state.relics.clear();
    run_state
        .relics
        .push(RelicState::new(RelicId::EternalFeather));
    run_state.master_deck = (0..10)
        .map(|uuid| CombatCard::for_scenario(CardId::Strike, uuid))
        .collect();
    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SelectMapNode(0)),
    ));
    assert_eq!(run_state.current_hp, 26);
    assert!(matches!(engine_state, EngineState::Campfire));

    let mut blocked = run_state_with_first_room(RoomType::RestRoom);
    blocked.current_hp = 20;
    blocked.max_hp = 80;
    blocked.relics.clear();
    blocked
        .relics
        .push(RelicState::new(RelicId::EternalFeather));
    blocked
        .relics
        .push(RelicState::new(RelicId::MarkOfTheBloom));
    blocked.master_deck = (0..10)
        .map(|uuid| CombatCard::for_scenario(CardId::Strike, uuid))
        .collect();
    let mut blocked_engine = EngineState::MapNavigation;
    let mut blocked_combat = None;

    assert!(tick_run(
        &mut blocked_engine,
        &mut blocked,
        &mut blocked_combat,
        Some(ClientInput::SelectMapNode(0)),
    ));
    assert_eq!(blocked.current_hp, 20);
    assert!(matches!(blocked_engine, EngineState::Campfire));
}

#[test]
fn run_level_blood_potion_uses_sacred_bark_toy_ornithopter_and_consumes_slot() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.current_hp = 10;
    run_state.max_hp = 80;
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::SacredBark));
    run_state
        .relics
        .push(RelicState::new(RelicId::ToyOrnithopter));
    run_state.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::BloodPotion,
        101,
    ))];
    run_state.emitted_events.clear();
    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        }),
    ));

    assert_eq!(run_state.current_hp, 47);
    assert!(run_state.potions[0].is_none());
    assert!(run_state.emitted_events.iter().any(|event| matches!(
        event,
        crate::state::selection::DomainEvent::HpChanged {
            delta: 32,
            source: DomainEventSource::Potion(crate::content::potions::PotionId::BloodPotion),
            ..
        }
    )));
    assert!(run_state.emitted_events.iter().any(|event| matches!(
        event,
        crate::state::selection::DomainEvent::HpChanged {
            delta: 5,
            source: DomainEventSource::Relic(RelicId::ToyOrnithopter),
            ..
        }
    )));
    assert!(run_state.emitted_events.iter().any(|event| matches!(
        event,
        crate::state::selection::DomainEvent::PotionLost {
            potion_id: crate::content::potions::PotionId::BloodPotion,
            slot: 0,
            source: DomainEventSource::Potion(crate::content::potions::PotionId::BloodPotion),
        }
    )));
}

#[test]
fn run_level_potion_discard_is_blocked_by_we_meet_again() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.event_state = Some(crate::state::events::EventState::new(
        crate::state::events::EventId::WeMeetAgain,
    ));
    run_state.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::FirePotion,
        101,
    ))];
    let mut engine_state = EngineState::EventRoom;
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::DiscardPotion(0)),
    ));

    assert_eq!(
        run_state.potions[0].as_ref().map(|potion| potion.id),
        Some(crate::content::potions::PotionId::FirePotion)
    );
}

#[test]
fn run_level_potion_discard_works_inside_reward_overlay() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::AncientPotion,
        101,
    ))];
    let mut engine_state = EngineState::reward_overlay(
        crate::state::rewards::RewardState::new(),
        EngineState::Shop(crate::state::shop::ShopState::new()),
    );
    let expected_surface = engine_state.clone();
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::DiscardPotion(0)),
    ));

    assert!(run_state.potions[0].is_none());
    assert_eq!(
        engine_state, expected_surface,
        "discarding a potion must not close the overlaid Cauldron reward screen"
    );
}

#[test]
fn run_level_potion_execution_respects_imported_affordance_flags() {
    let mut disabled_use = RunState::new(1, 0, false, "Ironclad");
    disabled_use.current_hp = 10;
    disabled_use.max_hp = 80;
    disabled_use.potions = vec![Some(
        crate::content::potions::Potion::with_affordance_truth(
            crate::content::potions::PotionId::BloodPotion,
            101,
            false,
            true,
            false,
        ),
    )];
    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut disabled_use,
        &mut combat_state,
        Some(ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        }),
    ));

    assert_eq!(disabled_use.current_hp, 10);
    assert!(
        disabled_use.potions[0].is_some(),
        "Java PotionPopUp checks potion.canUse before calling use()"
    );

    let mut disabled_discard = RunState::new(1, 0, false, "Ironclad");
    disabled_discard.potions = vec![Some(
        crate::content::potions::Potion::with_affordance_truth(
            crate::content::potions::PotionId::FirePotion,
            102,
            false,
            false,
            true,
        ),
    )];
    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut disabled_discard,
        &mut combat_state,
        Some(ClientInput::DiscardPotion(0)),
    ));

    assert!(
        disabled_discard.potions[0].is_some(),
        "Java PotionPopUp checks potion.canDiscard before destroying the slot"
    );
}

#[test]
fn run_level_entropic_brew_consumes_slot_and_refills_without_limited_filter() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state.potions = vec![
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::EntropicBrew,
            101,
        )),
        None,
        None,
    ];
    let mut engine_state = EngineState::RewardScreen(crate::state::rewards::RewardState::new());
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        }),
    ));

    assert_eq!(
        run_state
            .potions
            .iter()
            .filter(|slot| slot.is_some())
            .count(),
        3
    );
    assert!(run_state.emitted_events.iter().any(|event| matches!(
        event,
        crate::state::selection::DomainEvent::PotionLost {
            potion_id: crate::content::potions::PotionId::EntropicBrew,
            slot: 0,
            source: DomainEventSource::Potion(crate::content::potions::PotionId::EntropicBrew),
        }
    )));
    assert_eq!(
        run_state
            .emitted_events
            .iter()
            .filter(|event| matches!(
                event,
                crate::state::selection::DomainEvent::PotionObtained {
                    source: DomainEventSource::Potion(
                        crate::content::potions::PotionId::EntropicBrew
                    ),
                    ..
                }
            ))
            .count(),
        3
    );
}

#[test]
fn run_level_entropic_brew_with_sozu_consumes_without_generating_potions() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.current_hp = 10;
    run_state.max_hp = 80;
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::Sozu));
    run_state
        .relics
        .push(RelicState::new(RelicId::ToyOrnithopter));
    run_state.potions = vec![
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::EntropicBrew,
            101,
        )),
        None,
        None,
    ];
    let potion_rng_before = run_state.rng_pool.potion_rng.counter;
    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        }),
    ));

    assert!(run_state.potions.iter().all(|slot| slot.is_none()));
    assert_eq!(
        run_state.rng_pool.potion_rng.counter, potion_rng_before,
        "Java EntropicBrew non-combat Sozu branch flashes Sozu and does not call returnRandomPotion"
    );
    assert_eq!(
        run_state.current_hp, 15,
        "Java PotionPopUp still calls relic onUsePotion after EntropicBrew.use(), even when Sozu blocks potion generation"
    );
    assert!(!run_state.emitted_events.iter().any(|event| matches!(
        event,
        crate::state::selection::DomainEvent::PotionObtained { .. }
    )));
    assert!(run_state.emitted_events.iter().any(|event| matches!(
        event,
        crate::state::selection::DomainEvent::HpChanged {
            delta: 5,
            source: DomainEventSource::Relic(RelicId::ToyOrnithopter),
            ..
        }
    )));
}

#[test]
fn run_level_entropic_brew_can_refill_own_full_slot_with_new_entropic_instance() {
    let seed = first_seed_for_first_generated_entropic(false);
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state.rng_pool.potion_rng = StsRng::new(seed);
    run_state.potions = vec![
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::EntropicBrew,
            101,
        )),
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::FirePotion,
            102,
        )),
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::BlockPotion,
            103,
        )),
    ];
    let mut engine_state = EngineState::MapNavigation;
    let mut combat_state = None;

    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        }),
    ));

    let replacement = run_state.potions[0]
        .as_ref()
        .expect("first ObtainPotionEffect should refill the consumed slot");
    assert_eq!(
        replacement.id,
        crate::content::potions::PotionId::EntropicBrew
    );
    assert_ne!(
        replacement.uuid, 101,
        "same-id Entropic replacement must still be distinguishable as a new potion instance"
    );
    assert_eq!(
        run_state
            .potions
            .iter()
            .filter(|slot| slot.is_some())
            .count(),
        3,
        "remaining generated potion effects should fail because all slots are full again"
    );
    assert!(run_state.emitted_events.iter().any(|event| matches!(
        event,
        crate::state::selection::DomainEvent::PotionLost {
            potion_id: crate::content::potions::PotionId::EntropicBrew,
            slot: 0,
            source: DomainEventSource::Potion(crate::content::potions::PotionId::EntropicBrew),
        }
    )));
    assert!(run_state.emitted_events.iter().any(|event| matches!(
        event,
        crate::state::selection::DomainEvent::PotionObtained {
            potion_id: crate::content::potions::PotionId::EntropicBrew,
            slot: 0,
            source: DomainEventSource::Potion(crate::content::potions::PotionId::EntropicBrew),
        }
    )));
}

fn first_seed_for_first_generated_entropic(limited: bool) -> u64 {
    (1..100_000)
        .find(|seed| {
            let mut rng = StsRng::new(*seed);
            crate::content::potions::random_potion(
                &mut rng,
                crate::content::potions::PotionClass::Ironclad,
                limited,
            ) == crate::content::potions::PotionId::EntropicBrew
        })
        .expect("test fixture should find an Entropic Brew replacement seed")
}

#[test]
fn bottled_relic_on_equip_filters_selection_by_card_type_and_marks_uuid() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state.master_deck = vec![
        CombatCard::for_scenario(CardId::Bash, 101),
        CombatCard::for_scenario(CardId::Defend, 102),
        CombatCard::for_scenario(CardId::Inflame, 103),
    ];

    let next_state = run_state
        .obtain_relic_with_source(
            RelicId::BottledFlame,
            EngineState::MapNavigation,
            DomainEventSource::RewardScreen,
        )
        .expect("Bottled Flame should open a deck selection when an attack exists");

    let EngineState::RunPendingChoice(choice) = next_state else {
        panic!("Bottled Flame should return RunPendingChoice");
    };
    let request = choice.selection_request(&run_state);
    assert_eq!(request.reason, SelectionReason::BottleFlame);
    assert_eq!(
        request.targets,
        vec![SelectionTargetRef::CardLineage(
            crate::runtime::combat::CardLineageId::constructed_scenario(101)
        )]
    );

    let mut engine_state = EngineState::RunPendingChoice(choice);
    let mut combat_state = None;
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SubmitSelection(SelectionResolution {
            scope: SelectionScope::Deck,
            selected: vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(101)
            )],
        })),
    ));

    assert!(matches!(engine_state, EngineState::MapNavigation));
    assert_eq!(
        run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::BottledFlame)
            .map(|relic| relic.amount),
        Some(crate::test_support::scenario_lineage(101).to_runtime_i32_payload())
    );
}

#[test]
fn duplicate_selection_preserves_stat_equivalent_card_state_without_copying_bottle_attachment() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    let mut bottled = RelicState::new(RelicId::BottledFlame);
    bottled.amount = crate::test_support::scenario_lineage(101).to_runtime_i32_payload();
    run_state.relics.push(bottled);

    let mut original = CombatCard::for_scenario(CardId::RitualDagger, 101);
    original.upgrades = 2;
    original.misc_value = 17;
    original.base_damage_override = Some(23);
    original.base_block_override = Some(14);
    original.cost_modifier = -1;
    original.cost_for_turn = Some(0);
    original.free_to_play_once = true;
    original.base_damage_mut = 99;
    original.base_block_mut = 88;
    original.base_magic_num_mut = 77;
    original.multi_damage = smallvec::smallvec![1, 2, 3];
    original.exhaust_override = Some(true);
    original.retain_override = Some(true);
    original.energy_on_use = 5;
    run_state.master_deck = vec![original];

    let next_state = run_state
        .obtain_relic_with_source(
            RelicId::DollysMirror,
            EngineState::MapNavigation,
            DomainEventSource::RewardScreen,
        )
        .expect("Dolly's Mirror should open a deck selection");

    let mut engine_state = next_state;
    let mut combat_state = None;
    assert!(tick_run(
        &mut engine_state,
        &mut run_state,
        &mut combat_state,
        Some(ClientInput::SubmitSelection(SelectionResolution {
            scope: SelectionScope::Deck,
            selected: vec![SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(101)
            )],
        })),
    ));

    assert!(matches!(engine_state, EngineState::MapNavigation));
    assert_eq!(run_state.master_deck.len(), 2);
    let copied = run_state
        .master_deck
        .iter()
        .find(|card| {
            card.lineage != crate::runtime::combat::CardLineageId::constructed_scenario(101)
        })
        .expect("Dolly's Mirror should add a copied card");
    assert_eq!(copied.id, CardId::RitualDagger);
    assert_eq!(copied.upgrades, 2);
    assert_eq!(copied.misc_value, 17);
    assert_eq!(copied.base_damage_override, Some(23));
    assert_eq!(copied.base_block_override, Some(14));
    assert_eq!(copied.cost_modifier, -1);
    assert_eq!(copied.cost_for_turn, Some(0));
    assert!(copied.free_to_play_once);
    assert_eq!(copied.base_damage_mut, 0);
    assert_eq!(copied.base_block_mut, 0);
    assert_eq!(copied.base_magic_num_mut, 0);
    assert!(copied.multi_damage.is_empty());
    assert_eq!(copied.exhaust_override, None);
    assert_eq!(copied.retain_override, None);
    assert_eq!(copied.energy_on_use, 0);
    assert_ne!(copied.lineage, crate::test_support::scenario_lineage(101));
    assert_eq!(
        run_state
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::BottledFlame)
            .map(|relic| relic.amount),
        Some(crate::test_support::scenario_lineage(101).to_runtime_i32_payload()),
        "Java clears bottle flags on the copied card; Rust bottle attachment stays on original UUID"
    );
}

#[test]
fn genetic_algorithm_replays_observe_growth_before_the_second_use() {
    use crate::content::powers::{store, PowerId};
    use crate::runtime::combat::{Power, PowerPayload};
    for (power, upgrades, expected_block, expected_misc) in [
        (PowerId::EchoForm, 0, 72, 39),
        (PowerId::EchoForm, 1, 73, 41),
        (PowerId::Burst, 0, 72, 39),
    ] {
        let mut run = RunState::new(1, 20, false, "Defect");
        let mut genetic = CombatCard::for_scenario(CardId::GeneticAlgorithm, 801);
        genetic.misc_value = 35;
        genetic.upgrades = upgrades;
        run.master_deck = vec![genetic.clone()];
        let mut combat = crate::test_support::blank_test_combat();
        combat.meta.player_class = "Defect".into();
        combat.meta.master_deck_snapshot = run.master_deck.clone().into();
        combat.zones.hand = vec![genetic];
        combat.entities.monsters = vec![crate::test_support::test_monster(
            crate::content::monsters::EnemyId::JawWorm,
        )];
        store::set_powers_for(
            &mut combat,
            0,
            vec![Power {
                power_type: power,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        let mut engine = EngineState::CombatPlayerTurn;
        let mut active = Some(ActiveCombat::new(
            engine.clone(),
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        tick_run_active(
            &mut engine,
            &mut run,
            &mut active,
            Some(ClientInput::PlayCard {
                card_index: 0,
                target: None,
            }),
        );
        for _ in 0..30 {
            if engine == EngineState::CombatPlayerTurn {
                break;
            }
            tick_run_active(&mut engine, &mut run, &mut active, None);
        }
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        let combat = &active.as_ref().unwrap().combat_state;
        assert_eq!(combat.entities.player.block, expected_block, "{power:?}");
        assert_eq!(run.master_deck[0].misc_value, expected_misc);
        assert_eq!(combat.zones.exhaust_pile.len(), 1);
        assert_eq!(combat.zones.exhaust_pile[0].misc_value, expected_misc);
    }
}

#[test]
fn genetic_algorithm_growth_persists_at_the_active_decision_and_only_once() {
    let mut run = RunState::new(1, 20, false, "Defect");
    let genetic = CombatCard::for_scenario(CardId::GeneticAlgorithm, 801);
    let untouched = CombatCard::for_scenario(CardId::GeneticAlgorithm, 802);
    run.master_deck = vec![genetic.clone(), untouched];
    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.player_class = "Defect".into();
    combat.meta.master_deck_snapshot = run.master_deck.clone().into();
    combat.zones.hand = vec![genetic];
    combat.entities.monsters = vec![crate::test_support::test_monster(
        crate::content::monsters::EnemyId::JawWorm,
    )];
    let mut engine = EngineState::CombatPlayerTurn;
    let mut active = Some(ActiveCombat::new(
        engine.clone(),
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    tick_run_active(
        &mut engine,
        &mut run,
        &mut active,
        Some(ClientInput::PlayCard {
            card_index: 0,
            target: None,
        }),
    );
    for _ in 0..20 {
        if engine == EngineState::CombatPlayerTurn {
            break;
        }
        tick_run_active(&mut engine, &mut run, &mut active, None);
    }
    assert_eq!(engine, EngineState::CombatPlayerTurn);
    assert_eq!(run.master_deck[0].misc_value, 3);
    assert_eq!(run.master_deck[1].misc_value, 1);
    let state = &active.as_ref().unwrap().combat_state;
    assert_eq!(state.entities.player.block, 1);
    assert_eq!(state.zones.exhaust_pile[0].misc_value, 3);
    assert_eq!(state.meta.master_deck_snapshot[0].misc_value, 3);
    assert!(state.meta.meta_changes.is_empty());
    engine = EngineState::GameOver(crate::state::core::RunResult::Victory);
    finish_active_combat(&mut engine, &mut run, &mut active);
    assert_eq!(run.master_deck[0].misc_value, 3);
    assert_eq!(run.master_deck[1].misc_value, 1);
}

#[test]
fn combat_misc_meta_change_updates_matching_master_deck_card() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    let mut dagger = CombatCard::for_scenario(CardId::RitualDagger, 101);
    dagger.misc_value = 17;
    run_state.master_deck = vec![dagger];

    apply_combat_meta_change(
        &mut run_state,
        crate::runtime::combat::MetaChange::ModifyCardMisc {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(101),
            amount: 3,
        },
    );

    assert_eq!(
        run_state.master_deck[0].misc_value, 20,
        "Java RitualDaggerAction updates player.masterDeck before GetAllInBattleInstances"
    );
}

#[test]
fn combat_upgrade_meta_change_updates_matching_master_deck_card() {
    let mut run_state = RunState::new(1, 0, false, "Watcher");
    run_state.master_deck = vec![CombatCard::for_scenario(CardId::StrikeP, 201)];

    apply_combat_meta_change(
        &mut run_state,
        crate::runtime::combat::MetaChange::UpgradeMasterDeckCard {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(201),
        },
    );

    assert_eq!(
        run_state.master_deck[0].upgrades, 1,
        "Java LessonLearnedAction upgrades a random canUpgrade() card from player.masterDeck"
    );
}

#[test]
fn bottled_relic_uuid_counts_as_innate_during_combat_deck_initialization() {
    let mut state = crate::test_support::blank_test_combat();
    let mut bottle = RelicState::new(RelicId::BottledTornado);
    bottle.amount = crate::test_support::scenario_lineage(103).to_runtime_i32_payload();
    state.entities.player.add_relic(bottle);
    state.zones.draw_pile = (vec![
        CombatCard::for_scenario(CardId::Strike, 101),
        CombatCard::for_scenario(CardId::Defend, 102),
        CombatCard::for_scenario(CardId::Inflame, 103),
    ])
    .into();

    state.apply_java_initialize_deck_order_after_shuffle();

    assert_eq!(
        state.zones.draw_pile.first().map(|card| card.lineage),
        Some(crate::test_support::scenario_lineage(103)),
        "the card selected by Bottled Tornado must be handled by the same start-hand path as innate cards"
    );
}

#[test]
fn matryoshka_on_chest_open_adds_extra_relic_before_base_chest_relic() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::Matryoshka));
    run_state.common_relic_pool = vec![RelicId::Anchor];
    run_state.uncommon_relic_pool = vec![RelicId::Anchor];
    run_state.rare_relic_pool = vec![RelicId::Mango];
    let relic_rng_before = run_state.rng_pool.relic_rng.counter;

    let rewards = open_treasure_chest(
        &mut run_state,
        TreasureChestState {
            size: TreasureChestSize::Small,
            base_relic_tier: RelicTier::Rare,
            gold_reward_base_amount: None,
        },
    );

    let relic_rewards = rewards
        .items
        .iter()
        .filter_map(|item| match item {
            RewardItem::Relic { relic_id } => Some(*relic_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        relic_rewards,
        vec![RelicId::Anchor, RelicId::Mango],
        "Java Matryoshka.onChestOpen inserts its extra relic before AbstractChest adds the base chest relic"
    );
    let matryoshka = run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::Matryoshka)
        .expect("Matryoshka should remain owned");
    assert_eq!(matryoshka.counter, 1);
    assert!(!matryoshka.used_up);
    assert_eq!(
        run_state.rng_pool.relic_rng.counter,
        relic_rng_before + 1,
        "Java Matryoshka consumes relicRng only for randomBoolean(0.75)"
    );
}

#[test]
fn nloths_mask_on_chest_open_after_removes_first_relic_after_matryoshka_and_base_rewards() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::Matryoshka));
    run_state.relics.push(RelicState::new(RelicId::NlothsMask));
    run_state.common_relic_pool = vec![RelicId::Anchor];
    run_state.uncommon_relic_pool = vec![RelicId::Anchor];
    run_state.rare_relic_pool = vec![RelicId::Mango];

    let rewards = open_treasure_chest(
        &mut run_state,
        TreasureChestState {
            size: TreasureChestSize::Small,
            base_relic_tier: RelicTier::Rare,
            gold_reward_base_amount: None,
        },
    );

    let relic_rewards = rewards
        .items
        .iter()
        .filter_map(|item| match item {
            RewardItem::Relic { relic_id } => Some(*relic_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        relic_rewards,
        vec![RelicId::Mango],
        "Java N'loth's Mask runs after AbstractChest adds the base relic, so it removes Matryoshka's earlier extra relic first"
    );
    let mask = run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::NlothsMask)
        .expect("N'loth's Mask should remain owned");
    assert_eq!(mask.counter, -2);
    assert!(mask.used_up);
}

#[test]
fn nloths_mask_chest_removal_also_removes_sapphire_key_linked_to_removed_relic() {
    let mut items = vec![
        RewardItem::Relic {
            relic_id: RelicId::Mango,
        },
        RewardItem::SapphireKey,
    ];

    remove_one_relic_from_rewards_after_chest_open(&mut items);

    assert!(items.is_empty());
}

#[test]
fn nloths_mask_removes_matryoshka_relic_before_base_chest_key_pair() {
    let mut items = vec![
        RewardItem::Relic {
            relic_id: RelicId::Omamori,
        },
        RewardItem::Relic {
            relic_id: RelicId::Mango,
        },
        RewardItem::SapphireKey,
    ];

    remove_one_relic_from_rewards_after_chest_open(&mut items);

    assert_eq!(
        items,
        vec![
            RewardItem::Relic {
                relic_id: RelicId::Mango,
            },
            RewardItem::SapphireKey,
        ],
        "Java Matryoshka adds its relic during onChestOpen before the base chest relic/key pair"
    );
}

#[test]
fn implant_is_persisted_at_the_active_tick_and_never_obtained_twice() {
    use crate::runtime::{action::Action, combat::CombatPhase};
    let mut run = RunState::new(1, 20, false, "Ironclad");
    run.relics = [
        RelicId::DarkstonePeriapt,
        RelicId::CeramicFish,
        RelicId::BloodyIdol,
    ]
    .into_iter()
    .map(RelicState::new)
    .collect();
    run.current_hp = 30;
    run.max_hp = 80;
    run.gold = 100;
    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.master_deck_snapshot = run.master_deck.clone().into();
    combat.meta.master_deck_lineage_allocator = run.master_deck_lineage_allocator.clone();
    combat.entities.player.relics = run.relics.clone();
    combat.entities.player.current_hp = run.current_hp;
    combat.entities.player.max_hp = run.max_hp;
    combat.entities.player.gold = run.gold;
    combat.turn.current_phase = CombatPhase::PlayerTurn;
    let mass = crate::test_support::test_monster(crate::content::monsters::EnemyId::WrithingMass);
    let source = mass.id;
    combat.entities.monsters = vec![mass];
    combat.queue_action_back(Action::AddCardToMasterDeck {
        card_id: CardId::Parasite,
        source,
    });
    let before_len = run.master_deck.len();
    let mut engine = EngineState::CombatProcessing;
    let mut active = Some(ActiveCombat::new(
        engine.clone(),
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    tick_run_active(&mut engine, &mut run, &mut active, None);
    assert_eq!(run.master_deck.len(), before_len + 1);
    assert_eq!((run.max_hp, run.current_hp, run.gold), (86, 41, 109));
    assert!(active
        .as_ref()
        .unwrap()
        .combat_state
        .meta
        .meta_changes
        .is_empty());
    let lineage = run.master_deck.last().unwrap().lineage;
    let allocator = run.master_deck_lineage_allocator.clone();
    assert!(run.emitted_events.iter().any(|event| matches!(event,
        DomainEvent::CardObtained { card, source: DomainEventSource::CombatMonsterGrant { .. } } if card.lineage == lineage)));
    engine = EngineState::GameOver(crate::state::core::RunResult::Victory);
    finish_active_combat(&mut engine, &mut run, &mut active);
    assert_eq!(run.master_deck.len(), before_len + 1);
    assert_eq!((run.max_hp, run.current_hp, run.gold), (86, 41, 109));
    assert_eq!(run.master_deck_lineage_allocator, allocator);
}
