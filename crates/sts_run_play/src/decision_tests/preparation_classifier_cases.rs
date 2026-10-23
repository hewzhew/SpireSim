//! Controlled executable preparation cases for classifier evaluation.
//! These roots declare their construction; they carry no strategy labels.
use super::*;
use sts_oracle_learning::content::monsters::factory::EncounterId;
use sts_presentation::Locale;

fn deck(session: &mut RunControlSession, cards: &[(CardId, u8)]) {
    session.run_state.master_deck.clear();
    for &(card, upgrades) in cards {
        session
            .run_state
            .add_card_to_deck_with_upgrades(card, upgrades);
    }
}

fn route(session: &mut RunControlSession, current_y: i32, room: RoomType) {
    let graph = (0..15)
        .map(|y| {
            let mut node = MapRoomNode::new(0, y);
            node.class = Some(if y == current_y {
                room
            } else if y == 14 || y == 5 {
                RoomType::RestRoom
            } else if y == 6 {
                RoomType::MonsterRoomElite
            } else {
                RoomType::MonsterRoom
            });
            if y < 14 {
                node.edges.insert(MapEdge::new(0, y, 0, y + 1));
            }
            vec![node]
        })
        .collect();
    session.run_state.map = MapState::new(graph);
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = current_y;
    session.run_state.keys = [true; 3];
    session.run_state.boss_key = Some(EncounterId::SlimeBoss);
    session.run_state.boss_list = vec![EncounterId::SlimeBoss];
}

fn cases() -> Vec<(&'static str, &'static str, RunControlSession)> {
    use CardId::*;
    let mut armaments = fixture("reward");
    armaments.run_state.advance_act();
    armaments.run_state.advance_act();
    armaments.run_state.floor_num = 48;
    armaments.run_state.current_hp = 55;
    armaments.run_state.potions = vec![None, None];
    armaments
        .run_state
        .relics
        .push(RelicState::new(RelicId::FusionHammer));
    deck(
        &mut armaments,
        &[
            (AscendersBane, 0),
            (Strike, 1),
            (Defend, 1),
            (Bash, 1),
            (ShrugItOff, 1),
            (PommelStrike, 1),
            (Carnage, 1),
            (Disarm, 1),
            (DemonForm, 1),
            (LimitBreak, 0),
        ],
    );
    let cards = [Armaments, Strike, Clash]
        .map(|card| RewardCard::new(card, 0))
        .to_vec();
    let mut rewards = RewardState::new();
    rewards.items = vec![RewardItem::Card {
        cards: cards.clone(),
    }];
    rewards.pending_card_choice = Some(cards);
    rewards.pending_card_reward_index = Some(0);
    armaments.engine_state = EngineState::RewardScreen(rewards);
    route(&mut armaments, 13, RoomType::MonsterRoom);
    armaments.run_state.boss_key = Some(EncounterId::TimeEater);
    armaments.run_state.boss_list = vec![EncounterId::TimeEater];
    let mut upgraded = armaments.clone();
    let target = upgraded
        .run_state
        .master_deck
        .iter()
        .find(|card| card.id == LimitBreak)
        .unwrap()
        .lineage;
    upgraded.run_state.upgrade_card(target);

    let mut campfire = fixture("reward");
    campfire.engine_state = EngineState::Campfire;
    campfire.run_state.floor_num = 15;
    campfire.run_state.current_hp = 72;
    campfire.run_state.potions = vec![None, None];
    deck(
        &mut campfire,
        &[
            (AscendersBane, 0),
            (Strike, 1),
            (Defend, 1),
            (Bash, 1),
            (ShrugItOff, 1),
            (PommelStrike, 1),
            (Carnage, 0),
            (Disarm, 1),
            (FlameBarrier, 1),
        ],
    );
    route(&mut campfire, 14, RoomType::RestRoom);
    let mut hurt = campfire.clone();
    hurt.run_state.current_hp = 8;

    let mut shop = fixture("shop");
    shop.run_state.floor_num = 4;
    shop.run_state.current_hp = 22;
    shop.run_state.gold = 40;
    shop.run_state.potions = vec![Some(Potion::new(PotionId::SpeedPotion, 0)), None];
    deck(
        &mut shop,
        &[
            (AscendersBane, 0),
            (Strike, 0),
            (Strike, 0),
            (Strike, 0),
            (Anger, 0),
            (PommelStrike, 0),
            (Carnage, 0),
            (Inflame, 0),
            (Clothesline, 0),
            (Bash, 0),
        ],
    );
    let mut stock = ShopState::new();
    stock.potions = vec![ShopPotion {
        potion_id: PotionId::BlockPotion,
        price: 40,
        can_buy: true,
        blocked_reason: None,
    }];
    shop.engine_state = EngineState::Shop(stock);
    route(&mut shop, 3, RoomType::ShopRoom);
    let mut crowded = shop.clone();
    crowded.run_state.potions[1] = Some(Potion::new(PotionId::FearPotion, 1));

    vec![
        ("armaments-base", "limit-break-upgrade", armaments),
        ("armaments-upgraded", "limit-break-upgrade", upgraded),
        ("campfire-healthy", "campfire-hp", campfire),
        ("campfire-hurt", "campfire-hp", hurt),
        ("shop-empty", "potion-slot", shop),
        ("shop-full", "potion-slot", crowded),
    ]
}

fn interaction_cases() -> Vec<(&'static str, &'static str, RunControlSession)> {
    let roots = cases();
    let normal = roots[0].2.clone();
    let mut corruption = normal.clone();
    corruption
        .run_state
        .add_card_to_deck_with_upgrades(CardId::Corruption, 1);
    let mut recovery = corruption.clone();
    recovery
        .run_state
        .add_card_to_deck_with_upgrades(CardId::Exhume, 1);
    let mut shop = roots[4].2.clone();
    shop.run_state.gold = 95;
    let EngineState::Shop(ref mut stock) = shop.engine_state else {
        unreachable!()
    };
    stock.cards.push(ShopCard {
        card_id: CardId::ShrugItOff,
        upgrades: 0,
        price: 60,
        can_buy: true,
        blocked_reason: None,
    });
    let mut enough = shop.clone();
    enough.run_state.gold = 100;
    vec![
        ("upgrade-clear", "corruption-timing", normal),
        ("upgrade-corruption", "corruption-timing", corruption),
        ("upgrade-recovery", "corruption-recovery", recovery),
        ("shop-budget-95", "purchase-budget", shop),
        ("shop-budget-100", "purchase-budget", enough),
    ]
}

#[test]
fn paired_preparation_roots_expose_upgrade_hp_and_slot_differences() {
    let dir = TestDirectory::new();
    let mut readings = Vec::new();
    for (name, _, session) in cases() {
        let mut game = controller_with_session(&dir.0, name, &session);
        readings.push(
            game.decision_reading_v1(&[DecisionDetailV1::Options], None, Locale::En)
                .unwrap(),
        );
    }
    assert!(readings[0].view.contains("Limit Break"));
    assert!(!readings[0].view.contains("Limit Break+"));
    assert!(readings[1].view.contains("Limit Break+"));
    assert!(readings[0].options.iter().any(|row| matches!(
        row.candidate,
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::TakeCard {
                card: CardId::Armaments,
                ..
            }
        }
    )));
    assert!(readings[2].view.contains("HP 72/75"));
    assert!(readings[3].view.contains("HP 8/75"));
    let has_block_purchase = |reading: &RunPlayDecisionReadingV1| {
        reading.options.iter().any(|row| {
            matches!(
                row.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::BuyPotion {
                        potion: PotionId::BlockPotion,
                        ..
                    }
                }
            )
        })
    };
    assert!(has_block_purchase(&readings[4]));
    assert!(!has_block_purchase(&readings[5]));
    assert!(readings[5].view.contains("Block Potion 40g (slots full)"));
    assert!(readings[5].view.contains("base 12"));
    for (name, case_index, final_hp) in [("healthy-rest", 2, 75), ("hurt-rest", 3, 30)] {
        let mut game = controller_with_session(&dir.0, name, &cases()[case_index].2);
        let reading = game.decision_reading_v1(&[], None, Locale::En).unwrap();
        game.choose_bound_v1(&pick(&reading, |action| {
            matches!(action, TaskStrategicActionV1::Rest)
        }))
        .unwrap();
        assert!(game
            .decision_reading_v1(&[], None, Locale::En)
            .unwrap()
            .view
            .contains(&format!("HP {final_hp}/75")));
    }
    let session = &cases()[5].2;
    let mut game = controller_with_session(&dir.0, "shop-exchange", session);
    let reading = game
        .decision_reading_v1(&[DecisionDetailV1::Options], None, Locale::En)
        .unwrap();
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(
            action,
            TaskStrategicActionV1::DiscardRunPotion {
                potion: PotionId::SpeedPotion,
                ..
            }
        )
    }))
    .unwrap();
    let available = game.decision_reading_v1(&[], None, Locale::En).unwrap();
    assert!(has_block_purchase(&available));
    game.choose_bound_v1(&pick(&available, |action| {
        matches!(
            action,
            TaskStrategicActionV1::BuyPotion {
                potion: PotionId::BlockPotion,
                ..
            }
        )
    }))
    .unwrap();
    let purchased = game.decision_reading_v1(&[], None, Locale::En).unwrap();
    assert!(purchased.view.contains("Gold 0"));
    assert!(purchased.view.contains("P0 Block Potion, P1 Fear Potion"));
}

#[test]
#[ignore = "exports fresh constructed classifier workspaces to STS_PREPARATION_CASE_OUTPUT"]
fn export_paired_preparation_roots() {
    export_roots(cases());
}

#[test]
#[ignore = "exports a public shop budget root to STS_PREPARATION_CASE_OUTPUT"]
fn export_shop_budget_root() {
    let mut shop = cases().remove(4).2;
    shop.run_state.gold = 165;
    let EngineState::Shop(ref mut stock) = shop.engine_state else {
        unreachable!()
    };
    stock.cards = vec![
        ShopCard {
            card_id: CardId::DarkEmbrace,
            upgrades: 0,
            price: 170,
            can_buy: true,
            blocked_reason: None,
        },
        ShopCard {
            card_id: CardId::ShrugItOff,
            upgrades: 0,
            price: 80,
            can_buy: true,
            blocked_reason: None,
        },
    ];
    stock.relics = vec![ShopRelic {
        relic_id: RelicId::MembershipCard,
        price: 50,
        can_buy: true,
        blocked_reason: None,
    }];
    export_roots(vec![("shop-budget", "public-price-combinations", shop)]);
}

#[test]
fn preparation_interaction_roots_preserve_upgrade_and_purchase_conditions() {
    let dir = TestDirectory::new();
    let roots = interaction_cases();
    assert!(!roots[0]
        .2
        .run_state
        .master_deck
        .iter()
        .any(|card| card.id == CardId::Corruption));
    assert!(roots[1]
        .2
        .run_state
        .master_deck
        .iter()
        .any(|card| card.id == CardId::Corruption && card.upgrades == 1));
    assert!(roots[2]
        .2
        .run_state
        .master_deck
        .iter()
        .any(|card| card.id == CardId::Exhume && card.upgrades == 1));
    for (name, index, expected_gold, can_buy_block) in [
        ("budget-short", 3, 35, false),
        ("budget-enough", 4, 40, true),
    ] {
        let mut game = controller_with_session(&dir.0, name, &roots[index].2);
        let reading = game.decision_reading_v1(&[], None, Locale::En).unwrap();
        game.choose_bound_v1(&pick(&reading, |action| {
            matches!(
                action,
                TaskStrategicActionV1::BuyCard {
                    card: CardId::ShrugItOff,
                    ..
                }
            )
        }))
        .unwrap();
        let after = game.decision_reading_v1(&[], None, Locale::En).unwrap();
        assert!(after.view.contains(&format!("Gold {expected_gold}")));
        assert_eq!(
            after.options.iter().any(|row| matches!(
                row.candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::BuyPotion {
                        potion: PotionId::BlockPotion,
                        ..
                    }
                }
            )),
            can_buy_block
        );
    }
}

#[test]
#[ignore = "exports fresh constructed interaction cases to STS_PREPARATION_CASE_OUTPUT"]
fn export_preparation_interaction_roots() {
    export_roots(interaction_cases());
}

fn selection_cases() -> Vec<(&'static str, &'static str, RunControlSession)> {
    use CardId::*;
    let mut event = fixture("reward");
    event.run_state.advance_act();
    event.run_state.floor_num = 22;
    event.run_state.current_hp = 49;
    event.run_state.max_hp = 80;
    event.run_state.gold = 256;
    event.run_state.potions = vec![
        Some(Potion::new(PotionId::LiquidBronze, 0)),
        None,
        None,
        None,
    ];
    event.run_state.relics = [
        RelicId::BurningBlood,
        RelicId::ToyOrnithopter,
        RelicId::PeacePipe,
        RelicId::PotionBelt,
        RelicId::TinyHouse,
        RelicId::Boot,
    ]
    .map(RelicState::new)
    .to_vec();
    deck(
        &mut event,
        &[
            (AscendersBane, 0),
            (Bash, 1),
            (BloodForBlood, 1),
            (Combust, 0),
            (Rupture, 0),
            (Defend, 0),
            (Defend, 0),
            (Defend, 0),
            (Defend, 1),
            (DemonForm, 0),
            (Disarm, 0),
            (DualWield, 0),
            (GhostlyArmor, 0),
            (Headbutt, 0),
            (Headbutt, 0),
            (Hemokinesis, 0),
            (RitualDagger, 0),
            (Strike, 0),
            (Strike, 0),
            (Strike, 0),
            (Strike, 0),
            (Strike, 0),
            (Whirlwind, 0),
        ],
    );
    route(&mut event, 3, RoomType::EventRoom);
    event.run_state.boss_key = Some(EncounterId::Collector);
    event.run_state.boss_list = vec![EncounterId::Collector];
    event.engine_state = EngineState::EventRoom;
    event.run_state.event_state = Some(EventState::new(EventId::Duplicator));
    let mut target = event.clone();
    sts_oracle_learning::content::events::duplicator::handle_choice(
        &mut target.engine_state,
        &mut target.run_state,
        0,
    );
    let mut cage = fixture("empty_cage");
    route(&mut cage, 14, RoomType::MonsterRoom);
    let mut discounted = target.clone();
    let power = discounted
        .run_state
        .master_deck
        .iter_mut()
        .find(|card| card.id == DemonForm)
        .unwrap();
    power.upgrades = 1;
    power.cost_modifier = -2;
    discounted
        .run_state
        .relics
        .push(RelicState::new(RelicId::CursedKey));
    vec![
        ("duplicate-preparation", "duplicate-targets", event),
        ("duplicate-target", "duplicate-targets", target),
        ("purge-two-targets", "selection-count", cage),
        (
            "duplicate-discounted-target",
            "duplicate-targets",
            discounted,
        ),
    ]
}

#[test]
fn preparation_selection_roots_execute_exact_single_and_multiple_targets() {
    use crate::selector::{parse_action_selector_v1, resolve_action_selector_v1};
    let dir = TestDirectory::new();
    let roots = selection_cases();
    for (index, line, delta) in [
        (1, "select:Disarm+0#0", 1),
        (2, "select:Strike+0#1,Strike+0#0", -2),
        (3, "select:DemonForm+#0", 1),
    ] {
        let mut game = controller_with_session(&dir.0, roots[index].0, &roots[index].2);
        let decision = game
            .head()
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let selection =
            parse_action_selector_v1(line, "selection-case", sts_presentation::Locale::En).unwrap();
        let resolved =
            resolve_action_selector_v1(&decision, &selection, sts_presentation::Locale::En)
                .unwrap();
        let before = roots[index].2.run_state.master_deck.len() as i32;
        let reading = game.decision_reading_v1(&[], None, Locale::En).unwrap();
        let choice = RunPlayBoundChoiceV1 {
            source: reading.source.clone().unwrap(),
            choice: resolved,
        };
        game.choose_bound_v1(&choice).unwrap();
        let state = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        assert_eq!(state.run_state.master_deck.len() as i32, before + delta);
        if index == 3 {
            assert_eq!(
                state
                    .run_state
                    .master_deck
                    .iter()
                    .filter(|card| card.id == CardId::DemonForm
                        && card.upgrades == 1
                        && card.cost_modifier == -2)
                    .count(),
                2
            );
        }
    }
}

#[test]
fn preparation_selection_target_readings_preserve_real_instances() {
    use crate::run_selection::project_run_selection_target_v1;
    let dir = TestDirectory::new();
    for (index, cost, upgrades) in [(1, 3, 0), (3, 1, 1)] {
        let roots = selection_cases();
        let game = controller_with_session(&dir.0, roots[index].0, &roots[index].2);
        let decision = game
            .head()
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        let targets = decision
            .candidates
            .iter()
            .filter_map(|candidate| project_run_selection_target_v1(&decision, candidate).unwrap())
            .collect::<Vec<_>>();
        let form = targets
            .iter()
            .find(|target| target.card.card == CardId::DemonForm)
            .unwrap();
        assert_eq!(form.reading.current.persistent_cost, cost);
        assert_eq!(form.reading.current.definition.upgrades, upgrades);
        let armor = targets
            .iter()
            .find(|target| target.card.card == CardId::GhostlyArmor)
            .unwrap();
        assert_eq!(armor.reading.current.persistent_block, 10);
        assert!(armor.reading.current.definition.ethereal);
    }
}

#[test]
#[ignore = "exports fresh constructed selection cases to STS_PREPARATION_CASE_OUTPUT"]
fn export_preparation_selection_roots() {
    export_roots(selection_cases());
}

fn protection_cases() -> Vec<(&'static str, &'static str, RunControlSession)> {
    use CardId::*;
    let mut sparse = fixture("reward");
    sparse.run_state.advance_act();
    sparse.run_state.floor_num = 19;
    sparse.run_state.current_hp = 26;
    sparse.run_state.max_hp = 80;
    sparse.run_state.gold = 205;
    sparse.run_state.potions = vec![None, None];
    sparse.run_state.relics = [RelicId::BurningBlood, RelicId::Boot]
        .map(RelicState::new)
        .to_vec();
    // Reviewed public inventory shape; an explicit construction, not a replay.
    let inventory = [
        (AscendersBane, 0),
        (Strike, 0),
        (Strike, 0),
        (Strike, 0),
        (Strike, 0),
        (Strike, 0),
        (Defend, 0),
        (Defend, 0),
        (Defend, 0),
        (Defend, 0),
        (Bash, 0),
        (SwordBoomerang, 0),
        (SeverSoul, 0),
        (PerfectedStrike, 0),
        (Rampage, 0),
        (Anger, 0),
        (Cleave, 0),
        (Hemokinesis, 1),
        (PowerThrough, 1),
        (Bloodletting, 0),
        (Pummel, 0),
        (FireBreathing, 1),
        (TrueGrit, 0),
        (Brutality, 0),
        (Headbutt, 0),
        (Disarm, 0),
    ];
    deck(&mut sparse, &inventory);
    route(&mut sparse, 2, RoomType::MonsterRoom);
    sparse.run_state.boss_key = Some(EncounterId::Collector);
    sparse.run_state.boss_list = vec![EncounterId::Collector];
    let reward = |offers: [CardId; 3]| {
        let cards = offers.map(|card| RewardCard::new(card, 0)).to_vec();
        let mut rewards = RewardState::new();
        rewards.items = vec![RewardItem::Card {
            cards: cards.clone(),
        }];
        rewards.pending_card_choice = Some(cards);
        rewards.pending_card_reward_index = Some(0);
        EngineState::RewardScreen(rewards)
    };
    sparse.engine_state = reward([Pummel, Disarm, FlameBarrier]);
    let mut abundant = sparse.clone();
    let replacements = [
        (SwordBoomerang, PowerThrough, 1),
        (PerfectedStrike, PowerThrough, 1),
        (Rampage, Disarm, 1),
        (Anger, ShrugItOff, 1),
        (Cleave, ShrugItOff, 1),
        (Headbutt, BattleTrance, 1),
    ];
    let expanded = inventory.map(|(id, upgrades)| {
        replacements
            .iter()
            .find(|(from, _, _)| *from == id)
            .map_or((id, upgrades), |(_, to, upgrades)| (*to, *upgrades))
    });
    deck(&mut abundant, &expanded);
    let mut access = sparse.clone();
    access.engine_state = reward([FlameBarrier, ShrugItOff, DarkEmbrace]);
    vec![
        ("protection-sparse", "protection-quantity", sparse),
        ("protection-abundant", "protection-quantity", abundant),
        ("protection-access", "protection-vs-access", access),
    ]
}

#[test]
fn preparation_protection_roots_preserve_size_and_execute_addition_or_skip() {
    let dir = TestDirectory::new();
    let roots = protection_cases();
    for (index, expected_blocks, added) in [(0, 1, true), (1, 3, false)] {
        let (name, _, session) = &roots[index];
        assert_eq!(session.run_state.master_deck.len(), 26);
        assert_eq!(
            session
                .run_state
                .master_deck
                .iter()
                .filter(|card| card.id == CardId::PowerThrough)
                .count(),
            expected_blocks
        );
        let mut game = controller_with_session(&dir.0, name, session);
        let reading = game.decision_reading_v1(&[], None, Locale::En).unwrap();
        assert!(reading.view.contains("Connected paths to the first Elite"));
        assert!(reading
            .view
            .contains("First Elite: (0,6) Elite; approach: 2 Monster, 0 ?; Rest (0,5)"));
        assert_eq!(
            reading
                .options
                .iter()
                .filter(|row| matches!(
                    row.candidate,
                    TaskPublicCandidateV1::Strategic {
                        action: TaskStrategicActionV1::TakeCard { .. }
                    }
                ))
                .count(),
            3
        );
        game.choose_bound_v1(&pick(&reading, |action| {
            if added {
                matches!(
                    action,
                    TaskStrategicActionV1::TakeCard {
                        card: CardId::FlameBarrier,
                        ..
                    }
                )
            } else {
                matches!(action, TaskStrategicActionV1::SkipCardReward { .. })
            }
        }))
        .unwrap();
        let state = game
            .current_external_exact_checkpoint_v1()
            .unwrap()
            .into_session()
            .unwrap();
        assert_eq!(
            state.run_state.master_deck.len(),
            if added { 27 } else { 26 }
        );
    }
}

#[test]
#[ignore = "exports fresh constructed protection cases to STS_PREPARATION_CASE_OUTPUT"]
fn export_preparation_protection_roots() {
    export_roots(protection_cases());
}

fn attack_increment_cases() -> Vec<(&'static str, &'static str, RunControlSession)> {
    use CardId::*;
    let reward = |offers: [CardId; 3]| {
        let cards = offers.map(|card| RewardCard::new(card, 0)).to_vec();
        let mut rewards = RewardState::new();
        rewards.items = vec![RewardItem::Card {
            cards: cards.clone(),
        }];
        rewards.pending_card_choice = Some(cards);
        rewards.pending_card_reward_index = Some(0);
        EngineState::RewardScreen(rewards)
    };
    let mut sparse = protection_cases().remove(0).2;
    sparse.run_state.current_hp = 55;
    sparse.engine_state = reward([Pummel, BattleTrance, FlameBarrier]);
    let mut covered = sparse.clone();
    let mut changed_defends = 0;
    let inventory = sparse
        .run_state
        .master_deck
        .iter()
        .map(|card| {
            if card.id == Defend {
                changed_defends += 1;
                (
                    if changed_defends <= 2 {
                        PowerThrough
                    } else {
                        ShrugItOff
                    },
                    1,
                )
            } else if card.id == FireBreathing {
                (DarkEmbrace, 1)
            } else {
                (card.id, card.upgrades)
            }
        })
        .collect::<Vec<_>>();
    deck(&mut covered, &inventory);
    let mut all_attacks = covered.clone();
    all_attacks.engine_state = reward([Pummel, Headbutt, TwinStrike]);
    let mut early = fixture("reward");
    early.run_state.floor_num = 1;
    early.run_state.relics = [RelicId::BurningBlood, RelicId::Boot]
        .map(RelicState::new)
        .to_vec();
    early.run_state.potions = vec![None, None];
    deck(
        &mut early,
        &[
            (AscendersBane, 0),
            (Strike, 0),
            (Strike, 0),
            (Strike, 0),
            (Strike, 0),
            (Strike, 0),
            (Defend, 0),
            (Defend, 0),
            (Defend, 0),
            (Defend, 0),
            (Bash, 0),
        ],
    );
    early.engine_state = reward([Pummel, BattleTrance, FlameBarrier]);
    route(&mut early, 0, RoomType::MonsterRoom);
    vec![
        ("attack-access-sparse", "same-attack-suite", sparse),
        ("attack-access-covered", "same-attack-suite", covered),
        ("attack-only-covered", "same-attack-suite", all_attacks),
        ("attack-early", "first-transition-attack", early),
    ]
}

#[test]
fn preparation_attack_increment_roots_keep_the_owned_attack_suite_identical() {
    let roots = attack_increment_cases();
    let attacks = |session: &RunControlSession| {
        session
            .run_state
            .master_deck
            .iter()
            .filter(|card| {
                sts_oracle_learning::content::cards::get_card_definition(card.id).card_type
                    == sts_oracle_learning::content::cards::CardType::Attack
            })
            .map(|card| {
                (
                    card.id,
                    card.upgrades,
                    card.cost_modifier,
                    card.base_damage_override,
                )
            })
            .collect::<Vec<_>>()
    };
    let expected = attacks(&roots[0].2);
    assert_eq!(expected.len(), 15);
    for (_, _, session) in &roots[..3] {
        assert_eq!(session.run_state.master_deck.len(), 26);
        assert_eq!(attacks(session), expected);
        assert_eq!(session.run_state.current_hp, 55);
    }
    assert_eq!(attacks(&roots[3].2).len(), 6);
    assert_eq!(roots[3].2.run_state.master_deck.len(), 11);
    let dir = TestDirectory::new();
    let mut game = controller_with_session(&dir.0, roots[2].0, &roots[2].2);
    let reading = game.decision_reading_v1(&[], None, Locale::En).unwrap();
    game.choose_bound_v1(&pick(&reading, |action| {
        matches!(action, TaskStrategicActionV1::SkipCardReward { .. })
    }))
    .unwrap();
    let state = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(attacks(&state), expected);
    assert_eq!(state.run_state.master_deck.len(), 26);
}

#[test]
#[ignore = "exports fresh constructed attack-increment cases to STS_PREPARATION_CASE_OUTPUT"]
fn export_preparation_attack_increment_roots() {
    export_roots(attack_increment_cases());
}

fn export_roots(roots: Vec<(&str, &str, RunControlSession)>) {
    let root = PathBuf::from(
        std::env::var_os("STS_PREPARATION_CASE_OUTPUT").expect("explicit output directory"),
    );
    assert!(root.is_absolute());
    fs::create_dir(&root).expect("fresh output directory");
    let mut manifest = Vec::new();
    for (name, pair, session) in roots {
        let mut game = controller_with_session(&root, name, &session);
        let reading = game.decision_reading_v1(&[], None, Locale::En).unwrap();
        fs::write(root.join(format!("{name}-reading.txt")), reading.view).unwrap();
        manifest.push(
            serde_json::json!({ "case": name, "pair": pair, "session": root.join(name),
            "source": "explicitly constructed executable state, projected by current simulator",
            "ascension": 20, "goal": "heart-victory" }),
        );
    }
    fs::write(
        root.join("cases.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
}
