use super::*;
use crate::content::cards::{CardId, CardRarity};
use crate::content::relics::RelicId;

fn deck_ids(run: &RunState) -> Vec<CardId> {
    run.master_deck.iter().map(|card| card.id).collect()
}

fn card_rng_after_calls(count: u32) -> StsRng {
    let mut rng = StsRng::new(17);
    for _ in 0..count {
        let _ = rng.random(999);
    }
    rng
}

#[test]
fn note_for_yourself_pool_presence_matches_java_run_initialization_gate() {
    let a0 = RunState::new(1, 0, false, "Ironclad");
    assert!(a0
        .event_generator
        .one_time_event_pool
        .contains(&crate::state::events::EventId::NoteForYourself));

    let a1 = RunState::new(1, 1, false, "Ironclad");
    assert!(!a1
        .event_generator
        .one_time_event_pool
        .contains(&crate::state::events::EventId::NoteForYourself));

    let a15 = RunState::new(1, 15, false, "Ironclad");
    assert!(!a15
        .event_generator
        .one_time_event_pool
        .contains(&crate::state::events::EventId::NoteForYourself));
}

#[test]
fn starting_loadouts_use_class_specific_java_starter_decks() {
    let ironclad = RunState::new(1, 0, false, "Ironclad");
    assert_eq!(
        deck_ids(&ironclad),
        vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
        ]
    );
    assert_eq!(ironclad.relics[0].id, RelicId::BurningBlood);

    let silent = RunState::new(1, 0, false, "Silent");
    assert_eq!(
        deck_ids(&silent),
        vec![
            CardId::StrikeG,
            CardId::StrikeG,
            CardId::StrikeG,
            CardId::StrikeG,
            CardId::StrikeG,
            CardId::DefendG,
            CardId::DefendG,
            CardId::DefendG,
            CardId::DefendG,
            CardId::DefendG,
            CardId::Survivor,
            CardId::Neutralize,
        ]
    );
    assert_eq!(silent.relics[0].id, RelicId::SnakeRing);

    let defect = RunState::new(1, 0, false, "Defect");
    assert_eq!(
        deck_ids(&defect),
        vec![
            CardId::StrikeB,
            CardId::StrikeB,
            CardId::StrikeB,
            CardId::StrikeB,
            CardId::DefendB,
            CardId::DefendB,
            CardId::DefendB,
            CardId::DefendB,
            CardId::Zap,
            CardId::Dualcast,
        ]
    );
    assert_eq!(defect.relics[0].id, RelicId::CrackedCore);

    let watcher = RunState::new(1, 0, false, "Watcher");
    assert_eq!(
        deck_ids(&watcher),
        vec![
            CardId::StrikeP,
            CardId::StrikeP,
            CardId::StrikeP,
            CardId::StrikeP,
            CardId::DefendP,
            CardId::DefendP,
            CardId::DefendP,
            CardId::DefendP,
            CardId::Eruption,
            CardId::Vigilance,
        ]
    );
    assert_eq!(watcher.relics[0].id, RelicId::PureWater);
}

#[test]
fn ascenders_bane_precedes_the_java_starter_deck() {
    let ironclad = RunState::new(2, 20, false, "Ironclad");
    assert_eq!(
        deck_ids(&ironclad),
        vec![
            CardId::AscendersBane,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
        ]
    );
}

#[test]
fn event_curse_gate_excludes_java_unpurgeable_curses() {
    let mut run = RunState::new(4, 20, false, "Ironclad");

    assert!(run
        .master_deck
        .iter()
        .any(|card| card.id == CardId::AscendersBane));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::CurseOfTheBell, 7001));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Necronomicurse, 7002));
    assert!(
        !run.build_event_context(0, false).has_curses,
        "Java AbstractPlayer.isCursed excludes Ascender's Bane, Curse of the Bell, and Necronomicurse"
    );

    run.master_deck
        .push(CombatCard::for_scenario(CardId::Pain, 7003));
    assert!(
        run.build_event_context(0, false).has_curses,
        "an ordinary removable curse enables Fountain of Cleansing"
    );
}

#[test]
fn removing_parasite_runs_master_deck_removal_hook_before_deck_change_refresh() {
    let mut run = RunState::new(3, 0, false, "Ironclad");
    run.current_hp = 80;
    run.max_hp = 80;
    let parasite_uuid = 7001;
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Parasite, parasite_uuid));
    run.emitted_events.clear();

    run.remove_card_from_deck_with_source(
        crate::runtime::combat::CardLineageId::constructed_scenario(parasite_uuid),
        DomainEventSource::DeckMutation,
    );

    assert!(!run.master_deck.iter().any(|card| card.lineage
        == crate::runtime::combat::CardLineageId::constructed_scenario(parasite_uuid)));
    assert_eq!(run.max_hp, 77);
    assert_eq!(run.current_hp, 77);
    let events = run.take_emitted_events();
    assert!(events.iter().any(|event| matches!(
        event,
        DomainEvent::CardRemoved {
            card,
            source: DomainEventSource::DeckMutation,
        } if card.id == CardId::Parasite && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(parasite_uuid)
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        DomainEvent::MaxHpChanged {
            delta: -3,
            current_hp: 77,
            max_hp: 77,
            source: DomainEventSource::DeckMutation,
        }
    )));
}

#[test]
fn removing_necronomicurse_readds_directly_without_ordinary_obtain_hooks() {
    let mut run = RunState::new(5, 0, false, "Ironclad");
    run.current_hp = 80;
    run.max_hp = 80;
    run.gold = 100;
    run.relics
        .push(crate::content::relics::RelicState::new(RelicId::Omamori));
    run.relics.push(crate::content::relics::RelicState::new(
        RelicId::DarkstonePeriapt,
    ));
    run.relics.push(crate::content::relics::RelicState::new(
        RelicId::CeramicFish,
    ));
    let old_lineage = crate::test_support::scenario_lineage(7002);
    run.master_deck.push(CombatCard::with_lineage(
        CardId::Necronomicurse,
        old_lineage,
    ));
    run.emitted_events.clear();

    run.remove_card_from_deck_with_source(old_lineage, DomainEventSource::DeckMutation);

    let necronomicurses: Vec<_> = run
        .master_deck
        .iter()
        .filter(|card| card.id == CardId::Necronomicurse)
        .collect();
    assert_eq!(
        necronomicurses.len(),
        1,
        "Java NecronomicurseEffect directly re-adds one fresh Necronomicurse"
    );
    assert_ne!(necronomicurses[0].lineage, old_lineage);
    assert_eq!(run.max_hp, 80, "Darkstone must not fire on this re-add");
    assert_eq!(run.current_hp, 80);
    assert_eq!(run.gold, 100, "Ceramic Fish must not fire on this re-add");
    let omamori = run
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::Omamori)
        .expect("Omamori should be present");
    assert_eq!(omamori.counter, 2);
    assert!(!omamori.used_up);

    let events = run.take_emitted_events();
    assert!(events.iter().any(|event| matches!(
        event,
        DomainEvent::CardRemoved {
            card,
            source: DomainEventSource::DeckMutation,
        } if card.id == CardId::Necronomicurse && card.lineage == old_lineage
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        DomainEvent::CardObtained {
            card,
            source: DomainEventSource::DeckMutation,
        } if card.id == CardId::Necronomicurse && card.lineage != old_lineage
    )));
    assert!(!events.iter().any(|event| matches!(
        event,
        DomainEvent::GoldChanged { .. } | DomainEvent::MaxHpChanged { .. }
    )));
}

#[test]
fn ordinary_obtain_runs_relic_obtain_hooks_before_master_deck_add_like_java() {
    let mut run = RunState::new(7, 0, false, "Ironclad");
    run.relics.clear();
    run.relics.push(crate::content::relics::RelicState::new(
        RelicId::DarkstonePeriapt,
    ));
    run.current_hp = 50;
    run.max_hp = 80;
    run.emitted_events.clear();

    assert!(run.add_card_to_deck_with_upgrades_from(
        CardId::Regret,
        0,
        DomainEventSource::RewardScreen,
    ));

    let events = run.take_emitted_events();
    let max_hp_pos = events
        .iter()
        .position(|event| {
            matches!(
                event,
                DomainEvent::MaxHpChanged {
                    delta: 6,
                    source: DomainEventSource::RewardScreen,
                    ..
                }
            )
        })
        .expect("Darkstone Periapt should fire while obtaining a curse");
    let obtained_pos = events
        .iter()
        .position(|event| {
            matches!(
                event,
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::RewardScreen,
                } if card.id == CardId::Regret
            )
        })
        .expect("the curse should still be added after obtain hooks");

    assert!(
        max_hp_pos < obtained_pos,
        "Java ShowCardAndObtainEffect calls relic onObtainCard before Soul.obtain adds the card"
    );
}

#[test]
fn master_deck_upgrade_uses_java_card_upgrade_helper() {
    let mut run = RunState::new(9, 0, false, "Ironclad");
    let mut strike = CombatCard::for_scenario(CardId::Strike, 9101);
    strike.base_damage_override = Some(20);
    run.master_deck = vec![strike];
    run.emitted_events.clear();

    run.upgrade_card_with_source(
        crate::runtime::combat::CardLineageId::constructed_scenario(9101),
        DomainEventSource::DeckMutation,
    );

    assert_eq!(run.master_deck[0].upgrades, 1);
    assert_eq!(
        run.master_deck[0].base_damage_override,
        Some(23),
        "Java upgradeDamage adds the upgrade amount to the concrete card's current baseDamage"
    );
    assert!(run.take_emitted_events().iter().any(|event| matches!(
        event,
        DomainEvent::CardUpgraded {
            before,
            after,
            source: DomainEventSource::DeckMutation,
        } if before.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(9101) && before.upgrades == 0 && after.upgrades == 1
    )));
}

#[test]
fn init_relic_pools_shuffles_before_removing_owned_relics_like_java() {
    let mut run = RunState::new(33, 0, false, "Ironclad");
    run.relics
        .push(crate::content::relics::RelicState::new(RelicId::Anchor));

    let mut expected_common = crate::content::relics::build_relic_pool(
        crate::content::relics::RelicTier::Common,
        "Ironclad",
    );
    let mut expected_relic_rng = run.rng_pool.relic_rng.clone();
    crate::runtime::rng::shuffle_with_random_long(&mut expected_common, &mut expected_relic_rng);
    expected_common.retain(|&id| id != RelicId::Anchor);

    run.init_relic_pools();

    assert_eq!(
        run.common_relic_pool, expected_common,
        "Java initializeRelicList shuffles full pools before removing relicsToRemoveOnStart"
    );
    assert!(!run.common_relic_pool.contains(&RelicId::Anchor));
}

#[test]
fn event_random_card_helpers_use_java_rng_streams() {
    let mut run = RunState::new(11, 0, false, "Ironclad");
    let card_before = run.rng_pool.card_rng.counter;
    let misc_before = run.rng_pool.misc_rng.counter;
    let shuffle_before = run.rng_pool.shuffle_rng.counter;

    let _ = run.random_card_by_rarity(CardRarity::Rare);

    assert_eq!(
        run.rng_pool.card_rng.counter,
        card_before + 1,
        "Java AbstractDungeon.getCard(rarity) uses cardRng via CardGroup.getRandomCard(true)"
    );
    assert_eq!(
            run.rng_pool.misc_rng.counter, misc_before,
            "rarity card selection must not consume miscRng; Match and Keep uses miscRng later for board shuffle"
        );
    assert_eq!(run.rng_pool.shuffle_rng.counter, shuffle_before);

    let card_after = run.rng_pool.card_rng.counter;
    let misc_after = run.rng_pool.misc_rng.counter;
    let shuffle_after = run.rng_pool.shuffle_rng.counter;

    let _ = run.random_colorless_card(CardRarity::Uncommon);

    assert_eq!(run.rng_pool.card_rng.counter, card_after);
    assert_eq!(run.rng_pool.misc_rng.counter, misc_after);
    assert_eq!(
        run.rng_pool.shuffle_rng.counter,
        shuffle_after + 1,
        "Java returnColorlessCard(rarity) shuffles colorlessCardPool with shuffleRng.randomLong()"
    );
}

#[test]
fn boss_key_is_public_boss_while_boss_list_keeps_java_queue() {
    let mut run = RunState::new(7, 20, false, "Ironclad");
    run.act_num = 3;
    run.init_boss_list();

    assert_eq!(
        run.boss_key,
        run.boss_list.first().copied(),
        "Java setBoss(bossList[0]) publishes the current map boss"
    );
    assert_eq!(
        run.boss_list.len(),
        3,
        "Java keeps the full shuffled bossList; A20 double boss depends on the post-entry size"
    );

    let first = run.boss_key;
    assert_eq!(run.next_boss(), first);
    assert_eq!(run.boss_list.len(), 2);
    assert!(run.should_start_act3_double_boss());

    let second = run.reveal_next_boss_from_list();
    assert_eq!(second, run.boss_list.first().copied());
    assert_eq!(run.next_boss(), second);
    assert_eq!(run.boss_list.len(), 1);
    assert!(!run.should_start_act3_double_boss());
}

#[test]
fn final_act_initializes_shield_spear_and_heart_context() {
    use crate::content::monsters::factory::EncounterId;

    let mut run = RunState::new(7, 20, true, "Ironclad");
    run.current_hp = 20;
    run.max_hp = 80;
    run.potion_drop_chance_mod = 30;
    run.rng_pool.card_rng = card_rng_after_calls(501);
    let mut expected_card_rng = run.rng_pool.card_rng.clone();
    expected_card_rng.advance_counter_to(750);

    run.enter_final_act();

    assert_eq!(run.act_num, 4);
    assert_eq!(
        run.current_hp, 65,
        "TheEnding constructor also runs dungeonTransitionSetup and heals once"
    );
    assert_eq!(
        run.potion_drop_chance_mod, 0,
        "Java dungeonTransitionSetup resets AbstractRoom.blizzardPotionMod on Act 4 entry too"
    );
    assert_eq!(
        run.rng_pool.card_rng, expected_card_rng,
        "TheEnding constructor also applies the cardRng counter band alignment"
    );
    assert_eq!(run.elite_monster_list, vec![EncounterId::ShieldAndSpear; 3]);
    assert_eq!(run.monster_list, vec![EncounterId::ShieldAndSpear; 3]);
    assert_eq!(run.boss_list, vec![EncounterId::TheHeart; 3]);
    assert_eq!(run.boss_key, Some(EncounterId::TheHeart));
}

#[test]
fn advance_act_heals_once_like_java_dungeon_transition_setup() {
    for (asc, hp, max_hp, expected) in [
        (0, 20, 80, 80),
        (4, 25, 82, 82),
        (5, 20, 80, 65),
        (20, 25, 82, 68),
        (20, 42, 82, 72),
        (20, 79, 82, 81),
        (20, 82, 82, 82),
    ] {
        let mut run = RunState::new(7, asc, false, "Ironclad");
        run.current_hp = hp;
        run.max_hp = max_hp;
        let projected = RunState::dungeon_transition_hp(hp, max_hp, asc);
        run.advance_act();
        assert_eq!(run.current_hp, expected, "A{asc}, {hp}/{max_hp}");
        assert_eq!(projected, run.current_hp);
    }
}

#[test]
fn advance_act_aligns_card_rng_counter_like_java_dungeon_transition_setup() {
    for (counter_before, expected_counter_after) in [
        (0, 0),
        (1, 250),
        (249, 250),
        (250, 250),
        (251, 500),
        (499, 500),
        (500, 500),
        (501, 750),
        (749, 750),
        (750, 750),
        (800, 800),
    ] {
        let mut run = RunState::new(17, 0, false, "Ironclad");
        run.rng_pool.card_rng = card_rng_after_calls(counter_before);

        let mut expected = run.rng_pool.card_rng.clone();
        expected.advance_counter_to(expected_counter_after);

        run.advance_act();

        assert_eq!(
                run.rng_pool.card_rng, expected,
                "Java dungeonTransitionSetup aligns cardRng counter {counter_before} to {expected_counter_after} by consuming randomBoolean calls"
            );
    }
}

#[test]
fn advance_act_resets_potion_drop_chance_like_java_dungeon_transition_setup() {
    let mut run = RunState::new(7, 0, false, "Ironclad");
    run.potion_drop_chance_mod = -20;

    run.advance_act();

    assert_eq!(
        run.potion_drop_chance_mod, 0,
        "Java dungeonTransitionSetup resets AbstractRoom.blizzardPotionMod between acts"
    );
}

#[test]
fn advance_act_generates_encounters_before_map_like_java_constructors() {
    use crate::runtime::rng::begin_sts_rng_trace;
    let mut run = RunState::new(7, 20, true, "Ironclad");
    let trace = begin_sts_rng_trace().unwrap();
    run.advance_act();
    let calls = trace.finish().unwrap();
    let first_monster = calls
        .iter()
        .position(|call| call.owner == Some("run.monster"))
        .unwrap();
    let first_map = calls
        .iter()
        .position(|call| call.owner == Some("run.map"))
        .unwrap();
    assert!(first_monster < first_map);
    assert!(calls[first_map..]
        .iter()
        .all(|call| call.owner != Some("run.monster")));
    assert_eq!(calls.last().unwrap().owner, Some("floor.misc"));
    assert!(matches!(
        calls.last().unwrap().call,
        crate::runtime::rng::StsRngCall::RandomInt { range: 1, .. }
    ));
}

#[test]
fn boss_starter_upgrade_relics_require_matching_java_starter_relics() {
    let ironclad = RunState::new(1, 0, false, "Ironclad");
    assert!(ironclad.relic_can_spawn_now(RelicId::BlackBlood));
    assert!(!ironclad.relic_can_spawn_now(RelicId::RingOfTheSerpent));
    assert!(!ironclad.relic_can_spawn_now(RelicId::FrozenCore));
    assert!(!ironclad.relic_can_spawn_now(RelicId::HolyWater));

    let silent = RunState::new(1, 0, false, "Silent");
    assert!(silent.relic_can_spawn_now(RelicId::RingOfTheSerpent));
    assert!(!silent.relic_can_spawn_now(RelicId::BlackBlood));
    assert!(!silent.relic_can_spawn_now(RelicId::FrozenCore));
    assert!(!silent.relic_can_spawn_now(RelicId::HolyWater));

    let defect = RunState::new(1, 0, false, "Defect");
    assert!(defect.relic_can_spawn_now(RelicId::FrozenCore));
    assert!(!defect.relic_can_spawn_now(RelicId::BlackBlood));
    assert!(!defect.relic_can_spawn_now(RelicId::RingOfTheSerpent));
    assert!(!defect.relic_can_spawn_now(RelicId::HolyWater));

    let watcher = RunState::new(1, 0, false, "Watcher");
    assert!(watcher.relic_can_spawn_now(RelicId::HolyWater));
    assert!(!watcher.relic_can_spawn_now(RelicId::BlackBlood));
    assert!(!watcher.relic_can_spawn_now(RelicId::RingOfTheSerpent));
    assert!(!watcher.relic_can_spawn_now(RelicId::FrozenCore));
}
#[test]
fn relic_resource_old_coin_runs_gold_hooks_and_records_each_change_once() {
    let mut run = RunState::new(7, 0, false, "Ironclad");
    run.relics = vec![crate::content::relics::RelicState::new(RelicId::BloodyIdol)];
    run.gold = 10;
    run.current_hp = 20;
    run.max_hp = 80;
    run.emitted_events.clear();
    run.obtain_relic_with_source(
        RelicId::OldCoin,
        crate::state::core::EngineState::MapNavigation,
        DomainEventSource::RewardScreen,
    );
    assert_eq!((run.gold, run.current_hp), (310, 25));
    let events = run.take_emitted_events();
    let gold_events = events
        .iter()
        .filter(|event| matches!(event, DomainEvent::GoldChanged { .. }))
        .collect::<Vec<_>>();
    assert_eq!(gold_events.len(), 1);
    assert!(matches!(
        gold_events[0],
        DomainEvent::GoldChanged {
            delta: 300,
            new_total: 310,
            source: DomainEventSource::Relic(RelicId::OldCoin)
        }
    ));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, DomainEvent::HpChanged { .. }))
            .count(),
        1
    );
}

#[test]
fn relic_resource_gold_triggered_healing_respects_mark_of_the_bloom() {
    let mut run = RunState::new(7, 0, false, "Ironclad");
    run.relics = [RelicId::BloodyIdol, RelicId::MarkOfTheBloom]
        .into_iter()
        .map(crate::content::relics::RelicState::new)
        .collect();
    run.gold = 10;
    run.current_hp = 20;
    run.max_hp = 80;
    run.emitted_events.clear();
    assert_eq!(
        run.change_gold_with_source(5, DomainEventSource::RewardScreen),
        5
    );
    assert_eq!((run.gold, run.current_hp), (15, 20));
    assert!(!run
        .emitted_events
        .iter()
        .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
}

#[test]
fn relic_resource_max_hp_relics_do_not_bypass_mark_of_the_bloom() {
    for (relic, amount) in [
        (RelicId::Strawberry, 7),
        (RelicId::Pear, 10),
        (RelicId::Mango, 14),
        (RelicId::Waffle, 7),
    ] {
        let mut run = RunState::new(7, 0, false, "Ironclad");
        run.relics = vec![crate::content::relics::RelicState::new(
            RelicId::MarkOfTheBloom,
        )];
        run.current_hp = 20;
        run.max_hp = 80;
        run.emitted_events.clear();
        run.obtain_relic_at_with_source(
            relic,
            0,
            crate::state::core::EngineState::MapNavigation,
            DomainEventSource::RewardScreen,
        );
        assert_eq!((run.current_hp, run.max_hp), (20, 80 + amount), "{relic:?}");
        assert_eq!(
            run.emitted_events
                .iter()
                .filter(|event| matches!(event, DomainEvent::MaxHpChanged { .. }))
                .count(),
            1
        );
        assert!(!run
            .emitted_events
            .iter()
            .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
    }
}

#[test]
fn relic_resource_tiny_house_does_not_duplicate_its_explicit_max_hp_event() {
    let mut run = RunState::new(7, 0, false, "Ironclad");
    run.current_hp = 20;
    run.max_hp = 80;
    run.emitted_events.clear();
    run.obtain_boss_relic_choice_with_source(
        RelicId::TinyHouse,
        crate::state::core::EngineState::MapNavigation,
        DomainEventSource::RewardScreen,
    );
    assert_eq!((run.current_hp, run.max_hp), (25, 85));
    assert_eq!(
        run.emitted_events
            .iter()
            .filter(|event| matches!(event, DomainEvent::MaxHpChanged { .. }))
            .count(),
        1
    );
}

#[test]
fn relic_resource_ectoplasm_keeps_existing_gold_spendable_without_gain_hooks() {
    let mut run = RunState::new(7, 0, false, "Ironclad");
    run.relics = [RelicId::Ectoplasm, RelicId::BloodyIdol]
        .into_iter()
        .map(crate::content::relics::RelicState::new)
        .collect();
    run.current_hp = 20;
    run.max_hp = 80;
    run.gold = 120;
    run.emitted_events.clear();
    assert_eq!(
        run.change_gold_with_source(30, DomainEventSource::RewardScreen),
        0
    );
    run.obtain_relic_with_source(
        RelicId::OldCoin,
        crate::state::core::EngineState::MapNavigation,
        DomainEventSource::RewardScreen,
    );
    assert_eq!(
        run.change_gold_with_source(-25, DomainEventSource::Shop),
        -25
    );
    assert_eq!((run.gold, run.current_hp), (95, 20));
    let deltas = run
        .emitted_events
        .iter()
        .filter_map(|event| {
            if let DomainEvent::GoldChanged { delta, .. } = event {
                Some(*delta)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(deltas, [-25]);
    assert!(!run
        .emitted_events
        .iter()
        .any(|event| matches!(event, DomainEvent::HpChanged { .. })));
}
