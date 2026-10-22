use super::*;
use crate::decision_tests::{controller_with_session, fixture, TestDirectory};
use cards::CardId;
use relics::RelicId;
use sts_oracle_learning::content::identity::StableGameplayId;

fn card(id: CardId, ordinal: usize) -> TaskStrategicCardV1 {
    TaskStrategicCardV1 {
        ordinal,
        card: id,
        upgrades: 0,
        misc_value: 0,
        base_damage_override: None,
        base_block_override: None,
        cost_modifier: 0,
        bottled_by: None,
    }
}

#[test]
fn inventory_locales_preserve_identity_variants_and_original_permanent_faces() {
    let dir = TestDirectory::new();
    let game = controller_with_session(&dir.0, "inventory-locales", &fixture("reward"));
    let mut decision = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let TaskPublicObservationV1::Strategic { observation } = &mut decision.observation else {
        panic!()
    };
    observation.cards = [
        CardId::Strike,
        CardId::Strike,
        CardId::StrikeG,
        CardId::StrikeB,
        CardId::StrikeP,
        CardId::RitualDagger,
        CardId::RitualDagger,
        CardId::GeneticAlgorithm,
        CardId::Bash,
        CardId::Defend,
        CardId::SearingBlow,
        CardId::Anger,
        CardId::Anger,
        CardId::Impulse,
        CardId::Offering,
    ]
    .into_iter()
    .enumerate()
    .map(|(n, id)| card(id, n))
    .collect();
    observation.cards[5].misc_value = 45;
    observation.cards[6].misc_value = 75;
    observation.cards[7].misc_value = 32;
    observation.cards[8].base_damage_override = Some(23);
    observation.cards[8].cost_modifier = -1;
    observation.cards[9].base_block_override = Some(17);
    observation.cards[10].upgrades = 3;
    observation.cards[12].bottled_by = Some(RelicId::BottledFlame);
    let en = inventory_reading_decision_v1(decision.clone(), true, None, Locale::En).unwrap();
    let zh = inventory_reading_decision_v1(decision.clone(), true, None, Locale::ZhCn).unwrap();
    let identities = |reading: &InventoryReadingV1| {
        reading
            .cards
            .iter()
            .map(|row| (row.card, row.upgrades, row.count))
            .collect::<Vec<_>>()
    };
    assert_eq!(identities(&en), identities(&zh));
    assert_eq!(zh.cards.len(), 14);
    for reading in [&en, &zh] {
        for id in [
            CardId::Strike,
            CardId::StrikeG,
            CardId::StrikeB,
            CardId::StrikeP,
        ] {
            let rows: Vec<_> = reading.cards.iter().filter(|row| row.card == id).collect();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].count, if id == CardId::Strike { 2 } else { 1 });
            assert!(rows[0].view.contains(&format!("[{}]", id.stable_id())));
        }
        let daggers: Vec<_> = reading
            .cards
            .iter()
            .filter(|row| row.card == CardId::RitualDagger)
            .collect();
        assert_eq!(daggers.len(), 2);
        assert!(daggers[0].view.contains("45"));
        assert!(daggers[1].view.contains("75"));
        assert_eq!(
            reading
                .cards
                .iter()
                .filter(|row| row.card == CardId::Anger)
                .count(),
            2
        );
        assert_eq!(
            reading
                .cards
                .iter()
                .filter(|row| row.card == CardId::Impulse)
                .count(),
            1
        );
        for block in &reading.cards {
            assert!(reading.view.contains(&block.view));
        }
        for block in &reading.relics {
            assert!(reading.view.contains(&block.view));
        }
        assert!(reading.view.ends_with(&reading.resources));
    }
    assert!(zh.view.contains("永久卡组（15）"));
    assert!(en.view.contains("Deal 23 damage"));
    assert!(en.view.contains("Gain 17 Block"));
    assert!(en.view.contains("Gain 32 Block"));
    assert!(en.view.contains("Deal 27 damage"));
    assert!(zh.view.contains("灼热攻击+3"));
    assert!(zh.view.contains("瓶装火焰"));
    assert!(zh.view.contains("机制补充（英文）"));
    assert!(zh.view.contains("机制关联（英文）"));
    assert!(zh.view.contains("1 费"));
    assert!(zh.view.contains("基础格挡请求"));
    let plain = inventory_reading_decision_v1(decision, false, None, Locale::ZhCn).unwrap();
    assert!(!plain.view.contains("机制补充"));
    assert_eq!(identities(&plain), identities(&zh));
}

#[test]
fn inventory_combat_uses_master_snapshot_and_typed_bottle_lineage_without_advancing() {
    let dir = TestDirectory::new();
    let mut game = controller_with_session(
        &dir.0,
        "inventory-combat-locales",
        &fixture("combat_finish"),
    );
    let before = game.head().clone();
    let decision = game
        .head()
        .current_decision
        .as_ref()
        .unwrap()
        .public_decision_v1()
        .unwrap();
    let mut position = game.current_combat_position_for_lab_v1().unwrap();
    // Obtain the bottle binding from its real choice owner.
    let mut bottle_session = fixture("bottled_flame");
    bottle_session.run_state.master_deck.clear();
    bottle_session
        .run_state
        .add_card_to_deck(CardId::RitualDagger);
    bottle_session.run_state.add_card_to_deck(CardId::Bash);
    let mut bottle_game = controller_with_session(&dir.0, "bottle-binding", &bottle_session);
    let selection = bottle_game.decision_reading_v1(&[], None).unwrap();
    bottle_game
        .choose_bound_v1(&crate::decision::RunPlayBoundChoiceV1 {
            source: selection.source.unwrap(),
            choice: selection.options[0].choice.clone(),
        })
        .unwrap();
    let bottle_session = bottle_game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    let bottle = bottle_session
        .run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::BottledFlame)
        .unwrap()
        .clone();
    let mut dagger = bottle_session.run_state.master_deck[0].clone();
    dagger.misc_value = 75;
    let mut bash = bottle_session.run_state.master_deck[1].clone();
    bash.base_damage_override = Some(23);
    bash.cost_modifier = -1;
    position.combat.meta.master_deck_snapshot = vec![dagger.clone(), bash.clone()].into();
    position.combat.entities.player.relics.push(bottle);
    // Temporary combat fields and generated hand cards do not define inventory.
    bash.upgrades = 1;
    bash.cost_for_turn = Some(0);
    bash.base_damage_mut = 999;
    position.combat.zones.hand = vec![bash].into();
    for locale in [Locale::En, Locale::ZhCn] {
        let result =
            inventory_reading_decision_v1(decision.clone(), true, Some(position.clone()), locale)
                .unwrap();
        assert_eq!(result.cards.len(), 2);
        let dagger = result
            .cards
            .iter()
            .find(|row| row.card == CardId::RitualDagger)
            .unwrap();
        assert!(dagger.view.contains(locale.entity("relic", "BottledFlame")));
        assert!(dagger.view.contains("75"));
        let bash = result
            .cards
            .iter()
            .find(|row| row.card == CardId::Bash)
            .unwrap();
        assert_eq!(bash.upgrades, 0);
        assert!(bash.view.contains("23"));
        assert!(!bash.view.contains("999"));
        let _actual = render_inventory_v1(&mut game, true, locale).unwrap();
        assert_eq!(*game.head(), before);
    }
}

#[test]
fn inventory_relic_states_and_potions_use_the_requested_language() {
    for &id in RelicId::STABLE_IDS {
        let relic = RelicId::from_stable_id(id).unwrap();
        for flag in [false, true] {
            let en = relic_label_v1(relic, 2, flag, Locale::En);
            let zh = relic_label_v1(relic, 2, flag, Locale::ZhCn);
            assert!(en.starts_with(Locale::En.entity("relic", id)));
            assert!(zh.starts_with(Locale::ZhCn.entity("relic", id)));
            assert!(zh.contains("[2]"));
            if let Some(state) = relics::mechanics::trigger_state_description(relic, flag) {
                assert!(en.contains(state));
                assert!(!zh.contains(state));
            }
        }
    }
    assert!(relic_label_v1(RelicId::HappyFlower, 2, false, Locale::ZhCn)
        .contains("下个玩家回合能量 +1"));
    let dir = TestDirectory::new();
    let mut session = fixture("reward");
    session.run_state.potions = vec![
        Some(potions::Potion::new(potions::PotionId::SpeedPotion, 9001)),
        None,
    ];
    let mut game = controller_with_session(&dir.0, "inventory-potions-locales", &session);
    let before = game.head().clone();
    let zh = render_inventory_v1(&mut game, true, Locale::ZhCn).unwrap();
    assert!(
        !zh.contains("机制关联（英文）"),
        "empty rule sections should not have a heading"
    );
    assert!(zh.contains("P0 速度药水"));
    assert!(zh.contains("P1 空"));
    assert!(zh.contains(
        Locale::ZhCn
            .entry("game", "potionRule.SpeedPotion")
            .unwrap()
    ));
    assert_eq!(*game.head(), before);
}
