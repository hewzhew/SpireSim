use super::*;

#[test]
fn silent_upgrade_sensitive_play_paths_evaluate_from_card_definition() {
    fn upgraded(id: CardId, uuid: u32) -> CombatCard {
        let mut card = CombatCard::for_scenario(id, uuid);
        card.upgrades = 1;
        card
    }

    fn assert_damage(action: &Action, target: usize, amount: i32) {
        match action {
            Action::Damage(info) => {
                assert_eq!(info.target, target);
                assert_eq!(info.base, amount);
                assert_eq!(info.output, amount);
            }
            other => panic!("expected DamageAction, got {other:?}"),
        }
    }

    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    state.entities.monsters = vec![target];

    let adrenaline = resolve_card_play(
        CardId::Adrenaline,
        &state,
        &upgraded(CardId::Adrenaline, 900),
        None,
    );
    assert_eq!(adrenaline[0].action, Action::GainEnergy { amount: 2 });
    assert_eq!(adrenaline[1].action, Action::DrawCards(2));

    let backflip = resolve_card_play(
        CardId::Backflip,
        &state,
        &upgraded(CardId::Backflip, 901),
        None,
    );
    assert_eq!(
        backflip[0].action,
        Action::GainBlock {
            target: 0,
            amount: 8,
        }
    );
    assert_eq!(backflip[1].action, Action::DrawCards(2));

    let blade_dance = resolve_card_play(
        CardId::BladeDance,
        &state,
        &upgraded(CardId::BladeDance, 902),
        None,
    );
    match &blade_dance[0].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Shiv);
            assert_eq!(*amount, 4);
        }
        other => panic!("Blade Dance should generate constructed Shivs, got {other:?}"),
    }

    let bouncing_flask = resolve_card_play(
        CardId::BouncingFlask,
        &state,
        &upgraded(CardId::BouncingFlask, 903),
        None,
    );
    assert_eq!(
        bouncing_flask[0].action,
        Action::BouncingFlask {
            target: None,
            amount: 3,
            num_times: 4,
        }
    );

    let burst = resolve_card_play(CardId::Burst, &state, &upgraded(CardId::Burst, 904), None);
    assert_eq!(
        burst[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Burst,
            amount: 2,
        }
    );

    crate::content::powers::store::set_powers_for(
        &mut state,
        7,
        vec![Power {
            power_type: PowerId::Poison,
            instance_id: None,
            amount: 5,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let catalyst = resolve_card_play(
        CardId::Catalyst,
        &state,
        &upgraded(CardId::Catalyst, 905),
        Some(7),
    );
    assert_eq!(
        catalyst[0].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Poison,
            amount: 10,
        }
    );

    let cloak = resolve_card_play(
        CardId::CloakAndDagger,
        &state,
        &upgraded(CardId::CloakAndDagger, 906),
        None,
    );
    assert_eq!(
        cloak[0].action,
        Action::GainBlock {
            target: 0,
            amount: 6,
        }
    );
    match &cloak[1].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Shiv);
            assert_eq!(original.upgrades, 0);
            assert_eq!(*amount, 2);
        }
        other => panic!("Cloak and Dagger should generate constructed Shivs, got {other:?}"),
    }

    let dagger_throw = resolve_card_play(
        CardId::DaggerThrow,
        &state,
        &upgraded(CardId::DaggerThrow, 907),
        Some(7),
    );
    assert_damage(&dagger_throw[0].action, 7, 12);
    assert_eq!(dagger_throw[1].action, Action::DrawCards(1));

    let deadly_poison = resolve_card_play(
        CardId::DeadlyPoison,
        &state,
        &upgraded(CardId::DeadlyPoison, 908),
        Some(7),
    );
    assert_eq!(
        deadly_poison[0].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Poison,
            amount: 7,
        }
    );

    let footwork = resolve_card_play(
        CardId::Footwork,
        &state,
        &upgraded(CardId::Footwork, 909),
        None,
    );
    assert_eq!(
        footwork[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Dexterity,
            amount: 3,
        }
    );

    let noxious = resolve_card_play(
        CardId::NoxiousFumes,
        &state,
        &upgraded(CardId::NoxiousFumes, 910),
        None,
    );
    assert_eq!(
        noxious[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::NoxiousFumes,
            amount: 3,
        }
    );

    let poisoned_stab = resolve_card_play(
        CardId::PoisonedStab,
        &state,
        &upgraded(CardId::PoisonedStab, 911),
        Some(7),
    );
    assert_damage(&poisoned_stab[0].action, 7, 8);
    assert_eq!(
        poisoned_stab[1].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Poison,
            amount: 4,
        }
    );

    let survivor = resolve_card_play(
        CardId::Survivor,
        &state,
        &upgraded(CardId::Survivor, 912),
        None,
    );
    assert_eq!(
        survivor[0].action,
        Action::GainBlock {
            target: 0,
            amount: 11,
        }
    );
}

#[test]
fn discard_from_hand_auto_discards_all_when_hand_size_is_not_greater_than_amount() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 850),
        CombatCard::for_scenario(CardId::Defend, 851),
    ];

    crate::engine::action_handlers::cards::handle_discard_from_hand(2, false, false, &mut state);

    assert!(state.zones.hand.is_empty());
    assert_eq!(
        state
            .zones
            .discard_pile
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        vec![
            crate::runtime::combat::CardLineageId::constructed_scenario(851),
            crate::runtime::combat::CardLineageId::constructed_scenario(850),
        ],
        "Java DiscardAction repeatedly moves hand.getTopCard(); Rust discard_pile preserves Java CardGroup order with top at the end"
    );
    assert_eq!(
        state.pop_next_action(),
        None,
        "Java DiscardAction does not open a choice screen when hand.size() <= amount"
    );
}

#[test]
fn silent_common_batch_definitions_match_java_sources() {
    let cases = [
        (
            CardId::Deflect,
            "Deflect",
            CardType::Skill,
            0,
            0,
            4,
            0,
            CardTarget::SelfTarget,
            0,
            3,
            0,
        ),
        (
            CardId::QuickSlash,
            "Quick Slash",
            CardType::Attack,
            1,
            8,
            0,
            0,
            CardTarget::Enemy,
            4,
            0,
            0,
        ),
        (
            CardId::Slice,
            "Slice",
            CardType::Attack,
            0,
            6,
            0,
            0,
            CardTarget::Enemy,
            3,
            0,
            0,
        ),
        (
            CardId::FlyingKnee,
            "Flying Knee",
            CardType::Attack,
            1,
            8,
            0,
            0,
            CardTarget::Enemy,
            3,
            0,
            0,
        ),
        (
            CardId::DodgeAndRoll,
            "Dodge and Roll",
            CardType::Skill,
            1,
            0,
            4,
            0,
            CardTarget::SelfTarget,
            0,
            2,
            0,
        ),
        (
            CardId::SuckerPunch,
            "Sucker Punch",
            CardType::Attack,
            1,
            7,
            0,
            1,
            CardTarget::Enemy,
            2,
            0,
            1,
        ),
    ];

    let java_map = build_java_id_map();
    for (
        id,
        java_name,
        card_type,
        cost,
        damage,
        block,
        magic,
        target,
        upgrade_damage,
        upgrade_block,
        upgrade_magic,
    ) in cases
    {
        let def = get_card_definition(id);
        assert_eq!(def.name, java_name);
        assert_eq!(def.card_type, card_type);
        assert_eq!(def.rarity, CardRarity::Common);
        assert_eq!(def.cost, cost);
        assert_eq!(def.base_damage, damage);
        assert_eq!(def.base_block, block);
        assert_eq!(def.base_magic, magic);
        assert_eq!(def.target, target);
        assert_eq!(def.upgrade_damage, upgrade_damage);
        assert_eq!(def.upgrade_block, upgrade_block);
        assert_eq!(def.upgrade_magic, upgrade_magic);
        assert_eq!(java_id(id), java_name);
        assert_eq!(java_map.get(java_name), Some(&id));
    }
}

#[test]
fn silent_common_batch_runtime_actions_match_java_use_methods() {
    fn assert_damage(action: &Action, target: usize, amount: i32) {
        match action {
            Action::Damage(info) => {
                assert_eq!(info.source, 0);
                assert_eq!(info.target, target);
                assert_eq!(info.base, amount);
                assert_eq!(info.output, amount);
                assert_eq!(info.damage_type, DamageType::Normal);
            }
            other => panic!("expected DamageAction, got {other:?}"),
        }
    }

    let state = crate::test_support::blank_test_combat();

    let deflect = resolve_card_play(
        CardId::Deflect,
        &state,
        &CombatCard::for_scenario(CardId::Deflect, 880),
        None,
    );
    assert_eq!(
        deflect[0].action,
        Action::GainBlock {
            target: 0,
            amount: 4,
        }
    );

    let quick_slash = resolve_card_play(
        CardId::QuickSlash,
        &state,
        &CombatCard::for_scenario(CardId::QuickSlash, 881),
        Some(7),
    );
    assert_damage(&quick_slash[0].action, 7, 8);
    assert_eq!(quick_slash[1].action, Action::DrawCards(1));

    let slice = resolve_card_play(
        CardId::Slice,
        &state,
        &CombatCard::for_scenario(CardId::Slice, 882),
        Some(7),
    );
    assert_damage(&slice[0].action, 7, 6);

    let flying_knee = resolve_card_play(
        CardId::FlyingKnee,
        &state,
        &CombatCard::for_scenario(CardId::FlyingKnee, 883),
        Some(7),
    );
    assert_damage(&flying_knee[0].action, 7, 8);
    assert_eq!(
        flying_knee[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Energized,
            amount: 1,
        }
    );

    let dodge = resolve_card_play(
        CardId::DodgeAndRoll,
        &state,
        &CombatCard::for_scenario(CardId::DodgeAndRoll, 884),
        None,
    );
    assert_eq!(
        dodge[0].action,
        Action::GainBlock {
            target: 0,
            amount: 4,
        }
    );
    assert_eq!(
        dodge[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::NextTurnBlock,
            amount: 4,
        }
    );

    let sucker_punch = resolve_card_play(
        CardId::SuckerPunch,
        &state,
        &CombatCard::for_scenario(CardId::SuckerPunch, 885),
        Some(7),
    );
    assert_damage(&sucker_punch[0].action, 7, 7);
    assert_eq!(
        sucker_punch[1].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Weak,
            amount: 1,
        }
    );
}

#[test]
fn silent_economy_and_dash_cards_match_java_sources() {
    let outmaneuver = get_card_definition(CardId::Outmaneuver);
    assert_eq!(outmaneuver.name, "Outmaneuver");
    assert_eq!(outmaneuver.card_type, CardType::Skill);
    assert_eq!(outmaneuver.rarity, CardRarity::Common);
    assert_eq!(outmaneuver.cost, 1);
    assert_eq!(outmaneuver.base_magic, 2);
    assert_eq!(outmaneuver.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Outmaneuver), "Outmaneuver");

    let sneaky = get_card_definition(CardId::SneakyStrike);
    assert_eq!(sneaky.name, "Sneaky Strike");
    assert_eq!(sneaky.card_type, CardType::Attack);
    assert_eq!(sneaky.rarity, CardRarity::Common);
    assert_eq!(sneaky.cost, 2);
    assert_eq!(sneaky.base_damage, 12);
    assert_eq!(sneaky.upgrade_damage, 4);
    assert!(sneaky.tags.contains(&CardTag::Strike));
    assert_eq!(java_id(CardId::SneakyStrike), "Underhanded Strike");
    assert_eq!(
        build_java_id_map().get("Underhanded Strike"),
        Some(&CardId::SneakyStrike)
    );

    let dash = get_card_definition(CardId::Dash);
    assert_eq!(dash.name, "Dash");
    assert_eq!(dash.card_type, CardType::Attack);
    assert_eq!(dash.rarity, CardRarity::Uncommon);
    assert_eq!(dash.cost, 2);
    assert_eq!(dash.base_damage, 10);
    assert_eq!(dash.base_block, 10);
    assert_eq!(dash.upgrade_damage, 3);
    assert_eq!(dash.upgrade_block, 3);
}

#[test]
fn silent_economy_and_dash_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let outmaneuver = resolve_card_play(
        CardId::Outmaneuver,
        &state,
        &CombatCard::for_scenario(CardId::Outmaneuver, 886),
        None,
    );
    assert_eq!(
        outmaneuver[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Energized,
            amount: 2,
        }
    );

    let mut outmaneuver_plus = CombatCard::for_scenario(CardId::Outmaneuver, 887);
    outmaneuver_plus.upgrades = 1;
    let outmaneuver_plus_actions =
        resolve_card_play(CardId::Outmaneuver, &state, &outmaneuver_plus, None);
    assert_eq!(
        outmaneuver_plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Energized,
            amount: 3,
        }
    );

    let sneaky = resolve_card_play(
        CardId::SneakyStrike,
        &state,
        &CombatCard::for_scenario(CardId::SneakyStrike, 888),
        Some(7),
    );
    match &sneaky[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.output, 12);
        }
        other => panic!("Sneaky Strike first action should damage, got {other:?}"),
    }
    assert_eq!(
        sneaky[1].action,
        Action::GainEnergyIfDiscardedThisTurn { amount: 2 }
    );

    let mut no_discard_state = crate::test_support::blank_test_combat();
    no_discard_state.turn.set_energy(0);
    crate::engine::action_handlers::execute_action(
        Action::GainEnergyIfDiscardedThisTurn { amount: 2 },
        &mut no_discard_state,
    );
    assert_eq!(no_discard_state.turn.energy, 0);

    let mut discard_state = crate::test_support::blank_test_combat();
    discard_state.turn.set_energy(0);
    discard_state.turn.increment_cards_discarded();
    crate::engine::action_handlers::execute_action(
        Action::GainEnergyIfDiscardedThisTurn { amount: 2 },
        &mut discard_state,
    );
    assert_eq!(discard_state.turn.energy, 2);

    let dash = resolve_card_play(
        CardId::Dash,
        &state,
        &CombatCard::for_scenario(CardId::Dash, 889),
        Some(7),
    );
    assert_eq!(
        dash[0].action,
        Action::GainBlock {
            target: 0,
            amount: 10,
        }
    );
    match &dash[1].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.output, 10);
        }
        other => panic!("Dash second action should damage, got {other:?}"),
    }
}

#[test]
fn bane_uses_java_delayed_poison_check_for_second_hit() {
    let bane = get_card_definition(CardId::Bane);
    assert_eq!(bane.name, "Bane");
    assert_eq!(bane.card_type, CardType::Attack);
    assert_eq!(bane.rarity, CardRarity::Common);
    assert_eq!(bane.cost, 1);
    assert_eq!(bane.base_damage, 7);
    assert_eq!(bane.upgrade_damage, 3);
    assert_eq!(java_id(CardId::Bane), "Bane");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::Bane,
        &state,
        &CombatCard::for_scenario(CardId::Bane, 890),
        Some(7),
    );
    assert!(matches!(actions[0].action, Action::Damage(_)));
    let Action::BaneDamage(bane_info) = actions[1].action.clone() else {
        panic!("Bane second action should be Java BaneAction");
    };
    assert_eq!(bane_info.target, 7);
    assert_eq!(bane_info.output, 7);

    let mut no_poison_state = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 7;
    monster.current_hp = 20;
    no_poison_state.entities.monsters = vec![monster.clone()];
    crate::engine::action_handlers::execute_action(
        Action::BaneDamage(bane_info.clone()),
        &mut no_poison_state,
    );
    assert_eq!(
        no_poison_state.entities.monsters[0].current_hp, 20,
        "Java BaneAction does nothing if the target lacks Poison at execution time"
    );

    let mut poison_state = crate::test_support::blank_test_combat();
    poison_state.entities.monsters = vec![monster.clone()];
    poison_state.entities.power_db.insert(
        7,
        vec![Power {
            power_type: PowerId::Poison,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::BaneDamage(bane_info.clone()),
        &mut poison_state,
    );
    assert_eq!(poison_state.entities.monsters[0].current_hp, 13);

    let mut dead_poison_state = crate::test_support::blank_test_combat();
    let mut dead_monster = monster;
    dead_monster.current_hp = 0;
    dead_poison_state.entities.monsters = vec![dead_monster];
    dead_poison_state.entities.power_db.insert(
        7,
        vec![Power {
            power_type: PowerId::Poison,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::BaneDamage(bane_info),
        &mut dead_poison_state,
    );
    assert_eq!(
        dead_poison_state.entities.monsters[0].current_hp, 0,
        "Java BaneAction checks target.currentHealth > 0 before applying the second hit"
    );
}

#[test]
fn silent_discard_action_cards_match_java_sources() {
    let all_out = get_card_definition(CardId::AllOutAttack);
    assert_eq!(all_out.name, "All-Out Attack");
    assert_eq!(all_out.card_type, CardType::Attack);
    assert_eq!(all_out.rarity, CardRarity::Uncommon);
    assert_eq!(all_out.cost, 1);
    assert_eq!(all_out.base_damage, 10);
    assert!(all_out.is_multi_damage);
    assert_eq!(all_out.upgrade_damage, 4);
    assert_eq!(java_id(CardId::AllOutAttack), "All Out Attack");

    let concentrate = get_card_definition(CardId::Concentrate);
    assert_eq!(concentrate.name, "Concentrate");
    assert_eq!(concentrate.card_type, CardType::Skill);
    assert_eq!(concentrate.rarity, CardRarity::Uncommon);
    assert_eq!(concentrate.cost, 0);
    assert_eq!(concentrate.base_magic, 3);
    assert_eq!(concentrate.upgrade_magic, -1);

    let gamble = get_card_definition(CardId::CalculatedGamble);
    assert_eq!(gamble.name, "Calculated Gamble");
    assert_eq!(gamble.card_type, CardType::Skill);
    assert_eq!(gamble.rarity, CardRarity::Uncommon);
    assert_eq!(gamble.cost, 0);
    assert!(gamble.exhaust);
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::CalculatedGamble,
        891
    )));
    let mut gamble_plus = CombatCard::for_scenario(CardId::CalculatedGamble, 892);
    gamble_plus.upgrades = 1;
    assert!(
        !exhausts_when_played(&gamble_plus),
        "Calculated Gamble+ changes exhaust only"
    );
}

#[test]
fn silent_discard_action_cards_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::JawWorm);
    second.id = 8;
    state.entities.monsters = vec![first, second];

    let all_out = resolve_card_play(
        CardId::AllOutAttack,
        &state,
        &CombatCard::for_scenario(CardId::AllOutAttack, 893),
        None,
    );
    assert_eq!(all_out.len(), 2);
    assert_eq!(
        all_out[0].action,
        Action::DamageAllEnemies {
            source: 0,
            damages: smallvec::smallvec![10, 10],
            damage_type: DamageType::Normal,
            is_modified: true,
        }
    );
    assert_eq!(
        all_out[1].action,
        Action::DiscardFromHand {
            amount: 1,
            random: true,
            end_turn: false,
        }
    );

    let concentrate = resolve_card_play(
        CardId::Concentrate,
        &state,
        &CombatCard::for_scenario(CardId::Concentrate, 894),
        None,
    );
    assert_eq!(
        concentrate[0].action,
        Action::DiscardFromHand {
            amount: 3,
            random: false,
            end_turn: false,
        }
    );
    assert_eq!(concentrate[1].action, Action::GainEnergy { amount: 2 });

    let mut concentrate_plus = CombatCard::for_scenario(CardId::Concentrate, 895);
    concentrate_plus.upgrades = 1;
    let concentrate_plus_actions =
        resolve_card_play(CardId::Concentrate, &state, &concentrate_plus, None);
    assert_eq!(
        concentrate_plus_actions[0].action,
        Action::DiscardFromHand {
            amount: 2,
            random: false,
            end_turn: false,
        }
    );

    let gamble = resolve_card_play(
        CardId::CalculatedGamble,
        &state,
        &CombatCard::for_scenario(CardId::CalculatedGamble, 896),
        None,
    );
    assert_eq!(
        gamble[0].action,
        Action::CalculatedGamble { draw_extra: false }
    );

    let mut gamble_plus = CombatCard::for_scenario(CardId::CalculatedGamble, 897);
    gamble_plus.upgrades = 1;
    let gamble_plus_actions =
        resolve_card_play(CardId::CalculatedGamble, &state, &gamble_plus, None);
    assert_eq!(
        gamble_plus_actions[0].action,
        Action::CalculatedGamble { draw_extra: false },
        "Java CalculatedGamble.use passes false even when upgraded"
    );

    let mut runtime_state = crate::test_support::blank_test_combat();
    runtime_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 898),
        CombatCard::for_scenario(CardId::Defend, 899),
    ];
    crate::engine::action_handlers::execute_action(
        Action::CalculatedGamble { draw_extra: false },
        &mut runtime_state,
    );
    assert_eq!(
        runtime_state.pop_next_action(),
        Some(Action::DiscardFromHand {
            amount: 2,
            random: true,
            end_turn: false,
        })
    );
    assert_eq!(runtime_state.pop_next_action(), Some(Action::DrawCards(2)));
}

#[test]
fn silent_hand_conversion_cards_match_java_sources() {
    let storm = get_card_definition(CardId::StormOfSteel);
    assert_eq!(storm.name, "Storm of Steel");
    assert_eq!(storm.card_type, CardType::Skill);
    assert_eq!(storm.rarity, CardRarity::Rare);
    assert_eq!(storm.cost, 1);
    assert_eq!(storm.target, CardTarget::None);
    assert_eq!(java_id(CardId::StormOfSteel), "Storm of Steel");

    let unload = get_card_definition(CardId::Unload);
    assert_eq!(unload.name, "Unload");
    assert_eq!(unload.card_type, CardType::Attack);
    assert_eq!(unload.rarity, CardRarity::Rare);
    assert_eq!(unload.cost, 1);
    assert_eq!(unload.base_damage, 14);
    assert_eq!(unload.upgrade_damage, 4);
    assert_eq!(java_id(CardId::Unload), "Unload");
}

#[test]
fn silent_hand_conversion_cards_queue_java_execution_actions() {
    let state = crate::test_support::blank_test_combat();

    let storm = resolve_card_play(
        CardId::StormOfSteel,
        &state,
        &CombatCard::for_scenario(CardId::StormOfSteel, 900),
        None,
    );
    assert_eq!(storm[0].action, Action::BladeFury { upgraded: false });

    let mut storm_plus = CombatCard::for_scenario(CardId::StormOfSteel, 901);
    storm_plus.upgrades = 1;
    let storm_plus_actions = resolve_card_play(CardId::StormOfSteel, &state, &storm_plus, None);
    assert_eq!(
        storm_plus_actions[0].action,
        Action::BladeFury { upgraded: true }
    );

    let mut blade_state_base = crate::test_support::blank_test_combat();
    blade_state_base.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 905)];
    crate::engine::action_handlers::execute_action(
        Action::BladeFury { upgraded: false },
        &mut blade_state_base,
    );
    assert_eq!(
        blade_state_base.pop_next_action(),
        Some(Action::DiscardFromHand {
            amount: 1,
            random: false,
            end_turn: false,
        })
    );
    match blade_state_base.pop_next_action() {
        Some(Action::MakeConstructedCopyInHand { original, amount }) => {
            assert_eq!(original.id, CardId::Shiv);
            assert_eq!(original.upgrades, 0);
            assert_eq!(amount, 1);
        }
        other => panic!("Blade Fury should queue constructed Shiv copies, got {other:?}"),
    }

    let mut blade_state = crate::test_support::blank_test_combat();
    blade_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 902),
        CombatCard::for_scenario(CardId::Defend, 903),
    ];
    crate::engine::action_handlers::execute_action(
        Action::BladeFury { upgraded: true },
        &mut blade_state,
    );
    assert_eq!(
        blade_state.pop_next_action(),
        Some(Action::DiscardFromHand {
            amount: 2,
            random: false,
            end_turn: false,
        }),
        "Java BladeFuryAction addToTop's DiscardAction after MakeTempCardInHandAction, so discard executes first"
    );
    match blade_state.pop_next_action() {
        Some(Action::MakeConstructedCopyInHand { original, amount }) => {
            assert_eq!(original.id, CardId::Shiv);
            assert_eq!(original.upgrades, 1);
            assert_eq!(amount, 2);
        }
        other => panic!("Blade Fury+ should queue upgraded constructed Shiv copies, got {other:?}"),
    }

    let unload = resolve_card_play(
        CardId::Unload,
        &state,
        &CombatCard::for_scenario(CardId::Unload, 904),
        Some(7),
    );
    match &unload[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.output, 14);
        }
        other => panic!("Unload first action should damage, got {other:?}"),
    }
    assert_eq!(unload[1].action, Action::UnloadNonAttack);

    let mut unload_state = crate::test_support::blank_test_combat();
    unload_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 905),
        CombatCard::for_scenario(CardId::Defend, 906),
        CombatCard::for_scenario(CardId::Reflex, 907),
    ];
    crate::engine::action_handlers::execute_action(Action::UnloadNonAttack, &mut unload_state);
    assert_eq!(
        unload_state.pop_next_action(),
        Some(Action::DiscardCard { card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(907) }),
        "Java UnloadAction addToTop's DiscardSpecificCardAction while iterating hand; later non-attacks execute first"
    );
    assert_eq!(
        unload_state.pop_next_action(),
        Some(Action::DiscardCard {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(906)
        })
    );
    assert_eq!(unload_state.pop_next_action(), None);
}

#[test]
fn silent_direct_attack_batch_matches_java_sources() {
    let backstab = get_card_definition(CardId::Backstab);
    assert_eq!(backstab.name, "Backstab");
    assert_eq!(backstab.card_type, CardType::Attack);
    assert_eq!(backstab.rarity, CardRarity::Uncommon);
    assert_eq!(backstab.cost, 0);
    assert_eq!(backstab.base_damage, 11);
    assert_eq!(backstab.upgrade_damage, 4);
    assert!(backstab.exhaust);
    assert!(backstab.innate);
    assert_eq!(java_id(CardId::Backstab), "Backstab");

    let riddle = get_card_definition(CardId::RiddleWithHoles);
    assert_eq!(riddle.name, "Riddle With Holes");
    assert_eq!(riddle.card_type, CardType::Attack);
    assert_eq!(riddle.rarity, CardRarity::Uncommon);
    assert_eq!(riddle.cost, 2);
    assert_eq!(riddle.base_damage, 3);
    assert_eq!(riddle.upgrade_damage, 1);
    assert_eq!(java_id(CardId::RiddleWithHoles), "Riddle With Holes");

    let die = get_card_definition(CardId::DieDieDie);
    assert_eq!(die.name, "Die Die Die");
    assert_eq!(die.card_type, CardType::Attack);
    assert_eq!(die.rarity, CardRarity::Rare);
    assert_eq!(die.cost, 1);
    assert_eq!(die.base_damage, 13);
    assert_eq!(die.upgrade_damage, 4);
    assert!(die.is_multi_damage);
    assert!(die.exhaust);
    assert_eq!(java_id(CardId::DieDieDie), "Die Die Die");

    let finisher = get_card_definition(CardId::Finisher);
    assert_eq!(finisher.name, "Finisher");
    assert_eq!(finisher.card_type, CardType::Attack);
    assert_eq!(finisher.rarity, CardRarity::Uncommon);
    assert_eq!(finisher.cost, 1);
    assert_eq!(finisher.base_damage, 6);
    assert_eq!(finisher.upgrade_damage, 2);
    assert_eq!(java_id(CardId::Finisher), "Finisher");

    let flechettes = get_card_definition(CardId::Flechettes);
    assert_eq!(flechettes.name, "Flechettes");
    assert_eq!(flechettes.card_type, CardType::Attack);
    assert_eq!(flechettes.rarity, CardRarity::Uncommon);
    assert_eq!(flechettes.cost, 1);
    assert_eq!(flechettes.base_damage, 4);
    assert_eq!(flechettes.upgrade_damage, 2);
    assert_eq!(java_id(CardId::Flechettes), "Flechettes");

    let heel = get_card_definition(CardId::HeelHook);
    assert_eq!(heel.name, "Heel Hook");
    assert_eq!(heel.card_type, CardType::Attack);
    assert_eq!(heel.rarity, CardRarity::Uncommon);
    assert_eq!(heel.cost, 1);
    assert_eq!(heel.base_damage, 5);
    assert_eq!(heel.upgrade_damage, 3);
    assert_eq!(java_id(CardId::HeelHook), "Heel Hook");

    let expertise = get_card_definition(CardId::Expertise);
    assert_eq!(expertise.name, "Expertise");
    assert_eq!(expertise.card_type, CardType::Skill);
    assert_eq!(expertise.rarity, CardRarity::Uncommon);
    assert_eq!(expertise.cost, 1);
    assert_eq!(expertise.base_magic, 6);
    assert_eq!(expertise.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Expertise), "Expertise");

    let escape_plan = get_card_definition(CardId::EscapePlan);
    assert_eq!(escape_plan.name, "Escape Plan");
    assert_eq!(escape_plan.card_type, CardType::Skill);
    assert_eq!(escape_plan.rarity, CardRarity::Uncommon);
    assert_eq!(escape_plan.cost, 0);
    assert_eq!(escape_plan.base_block, 3);
    assert_eq!(escape_plan.upgrade_block, 2);
    assert_eq!(java_id(CardId::EscapePlan), "Escape Plan");

    let eviscerate = get_card_definition(CardId::Eviscerate);
    assert_eq!(eviscerate.name, "Eviscerate");
    assert_eq!(eviscerate.card_type, CardType::Attack);
    assert_eq!(eviscerate.rarity, CardRarity::Uncommon);
    assert_eq!(eviscerate.cost, 3);
    assert_eq!(eviscerate.base_damage, 7);
    assert_eq!(eviscerate.upgrade_damage, 2);
    assert_eq!(java_id(CardId::Eviscerate), "Eviscerate");

    let predator = get_card_definition(CardId::Predator);
    assert_eq!(predator.name, "Predator");
    assert_eq!(predator.card_type, CardType::Attack);
    assert_eq!(predator.rarity, CardRarity::Uncommon);
    assert_eq!(predator.cost, 2);
    assert_eq!(predator.base_damage, 15);
    assert_eq!(predator.upgrade_damage, 5);
    assert_eq!(java_id(CardId::Predator), "Predator");

    let accuracy = get_card_definition(CardId::Accuracy);
    assert_eq!(accuracy.name, "Accuracy");
    assert_eq!(accuracy.card_type, CardType::Power);
    assert_eq!(accuracy.rarity, CardRarity::Uncommon);
    assert_eq!(accuracy.cost, 1);
    assert_eq!(accuracy.base_magic, 4);
    assert_eq!(accuracy.upgrade_magic, 2);
    assert_eq!(java_id(CardId::Accuracy), "Accuracy");

    let caltrops = get_card_definition(CardId::Caltrops);
    assert_eq!(caltrops.name, "Caltrops");
    assert_eq!(caltrops.card_type, CardType::Power);
    assert_eq!(caltrops.rarity, CardRarity::Uncommon);
    assert_eq!(caltrops.cost, 1);
    assert_eq!(caltrops.base_magic, 3);
    assert_eq!(caltrops.upgrade_magic, 2);
    assert_eq!(java_id(CardId::Caltrops), "Caltrops");

    let infinite = get_card_definition(CardId::InfiniteBlades);
    assert_eq!(infinite.name, "Infinite Blades");
    assert_eq!(infinite.card_type, CardType::Power);
    assert_eq!(infinite.rarity, CardRarity::Uncommon);
    assert_eq!(infinite.cost, 1);
    assert!(!infinite.innate);
    let mut infinite_plus = CombatCard::for_scenario(CardId::InfiniteBlades, 927);
    infinite_plus.upgrades = 1;
    assert!(is_innate_card(&infinite_plus));
    assert_eq!(java_id(CardId::InfiniteBlades), "Infinite Blades");

    let masterful = get_card_definition(CardId::MasterfulStab);
    assert_eq!(masterful.name, "Masterful Stab");
    assert_eq!(masterful.card_type, CardType::Attack);
    assert_eq!(masterful.rarity, CardRarity::Uncommon);
    assert_eq!(masterful.cost, 0);
    assert_eq!(masterful.base_damage, 12);
    assert_eq!(masterful.upgrade_damage, 4);
    assert_eq!(java_id(CardId::MasterfulStab), "Masterful Stab");

    assert_eq!(build_java_id_map().get("Backstab"), Some(&CardId::Backstab));
    assert_eq!(
        build_java_id_map().get("Riddle With Holes"),
        Some(&CardId::RiddleWithHoles)
    );
    assert_eq!(
        build_java_id_map().get("Die Die Die"),
        Some(&CardId::DieDieDie)
    );
    assert_eq!(build_java_id_map().get("Finisher"), Some(&CardId::Finisher));
    assert_eq!(
        build_java_id_map().get("Flechettes"),
        Some(&CardId::Flechettes)
    );
    assert_eq!(
        build_java_id_map().get("Heel Hook"),
        Some(&CardId::HeelHook)
    );
    assert_eq!(
        build_java_id_map().get("Expertise"),
        Some(&CardId::Expertise)
    );
    assert_eq!(
        build_java_id_map().get("Escape Plan"),
        Some(&CardId::EscapePlan)
    );
    assert_eq!(
        build_java_id_map().get("Eviscerate"),
        Some(&CardId::Eviscerate)
    );
    assert_eq!(build_java_id_map().get("Predator"), Some(&CardId::Predator));
    assert_eq!(build_java_id_map().get("Accuracy"), Some(&CardId::Accuracy));
    assert_eq!(build_java_id_map().get("Caltrops"), Some(&CardId::Caltrops));
    assert_eq!(
        build_java_id_map().get("Infinite Blades"),
        Some(&CardId::InfiniteBlades)
    );
    assert_eq!(
        build_java_id_map().get("Masterful Stab"),
        Some(&CardId::MasterfulStab)
    );
}

#[test]
fn character_reward_pools_preserve_java_hashmap_runtime_order_for_implemented_cards() {
    assert_eq!(
        IRONCLAD_COMMON_POOL,
        &[
            CardId::Anger,
            CardId::Cleave,
            CardId::Warcry,
            CardId::Flex,
            CardId::IronWave,
            CardId::BodySlam,
            CardId::TrueGrit,
            CardId::ShrugItOff,
            CardId::Clash,
            CardId::ThunderClap,
            CardId::PommelStrike,
            CardId::TwinStrike,
            CardId::Clothesline,
            CardId::Armaments,
            CardId::Havoc,
            CardId::Headbutt,
            CardId::WildStrike,
            CardId::HeavyBlade,
            CardId::PerfectedStrike,
            CardId::SwordBoomerang,
        ]
    );
    assert_eq!(
        IRONCLAD_UNCOMMON_POOL,
        &[
            CardId::SpotWeakness,
            CardId::Inflame,
            CardId::PowerThrough,
            CardId::DualWield,
            CardId::InfernalBlade,
            CardId::RecklessCharge,
            CardId::Hemokinesis,
            CardId::Intimidate,
            CardId::BloodForBlood,
            CardId::FlameBarrier,
            CardId::Pummel,
            CardId::BurningPact,
            CardId::Metallicize,
            CardId::Shockwave,
            CardId::Rampage,
            CardId::SeverSoul,
            CardId::Whirlwind,
            CardId::Combust,
            CardId::DarkEmbrace,
            CardId::SeeingRed,
            CardId::Disarm,
            CardId::FeelNoPain,
            CardId::Rage,
            CardId::Entrench,
            CardId::Sentinel,
            CardId::BattleTrance,
            CardId::SearingBlow,
            CardId::SecondWind,
            CardId::Rupture,
            CardId::Bloodletting,
            CardId::Carnage,
            CardId::Dropkick,
            CardId::FireBreathing,
            CardId::GhostlyArmor,
            CardId::Uppercut,
            CardId::Evolve,
        ]
    );
    assert_eq!(
        IRONCLAD_RARE_POOL,
        &[
            CardId::Immolate,
            CardId::Offering,
            CardId::Exhume,
            CardId::Reaper,
            CardId::Brutality,
            CardId::Juggernaut,
            CardId::Impervious,
            CardId::Berserk,
            CardId::FiendFire,
            CardId::Barricade,
            CardId::Corruption,
            CardId::LimitBreak,
            CardId::Feed,
            CardId::Bludgeon,
            CardId::DemonForm,
            CardId::DoubleTap,
        ]
    );
    assert_eq!(
        SILENT_COMMON_POOL,
        &[
            CardId::CloakAndDagger,
            CardId::SneakyStrike,
            CardId::DeadlyPoison,
            CardId::DaggerSpray,
            CardId::Bane,
            CardId::BladeDance,
            CardId::Deflect,
            CardId::DaggerThrow,
            CardId::PoisonedStab,
            CardId::Acrobatics,
            CardId::QuickSlash,
            CardId::Slice,
            CardId::Backflip,
            CardId::Outmaneuver,
            CardId::Prepared,
            CardId::PiercingWail,
            CardId::SuckerPunch,
            CardId::DodgeAndRoll,
            CardId::FlyingKnee,
        ]
    );
    assert_eq!(
        SILENT_UNCOMMON_POOL,
        &[
            CardId::CripplingPoison,
            CardId::LegSweep,
            CardId::Catalyst,
            CardId::Tactician,
            CardId::Expertise,
            CardId::Choke,
            CardId::Caltrops,
            CardId::Blur,
            CardId::Setup,
            CardId::EndlessAgony,
            CardId::RiddleWithHoles,
            CardId::Skewer,
            CardId::CalculatedGamble,
            CardId::EscapePlan,
            CardId::Finisher,
            CardId::WellLaidPlans,
            CardId::Terror,
            CardId::HeelHook,
            CardId::NoxiousFumes,
            CardId::InfiniteBlades,
            CardId::Reflex,
            CardId::Eviscerate,
            CardId::Dash,
            CardId::Backstab,
            CardId::BouncingFlask,
            CardId::Concentrate,
            CardId::Flechettes,
            CardId::MasterfulStab,
            CardId::Accuracy,
            CardId::Footwork,
            CardId::Distraction,
            CardId::AllOutAttack,
            CardId::Predator,
        ]
    );
    assert_eq!(
        SILENT_RARE_POOL,
        &[
            CardId::GrandFinale,
            CardId::AThousandCuts,
            CardId::GlassKnife,
            CardId::StormOfSteel,
            CardId::BulletTime,
            CardId::AfterImage,
            CardId::Unload,
            CardId::Nightmare,
            CardId::ToolsOfTheTrade,
            CardId::WraithForm,
            CardId::Burst,
            CardId::Doppelganger,
            CardId::Envenom,
            CardId::Adrenaline,
            CardId::DieDieDie,
            CardId::PhantasmalKiller,
            CardId::Malaise,
            CardId::CorpseExplosion,
            CardId::Alchemize,
        ]
    );
    assert_eq!(
        COLORLESS_UNCOMMON_POOL,
        &[
            CardId::DarkShackles,
            CardId::PanicButton,
            CardId::Trip,
            CardId::DramaticEntrance,
            CardId::Impatience,
            CardId::Blind,
            CardId::BandageUp,
            CardId::DeepBreath,
            CardId::FlashOfSteel,
            CardId::Forethought,
            CardId::Enlightenment,
            CardId::Purity,
            CardId::Panacea,
            CardId::Discovery,
            CardId::Finesse,
            CardId::GoodInstincts,
            CardId::SwiftStrike,
            CardId::JackOfAllTrades,
            CardId::MindBlast,
            CardId::Madness,
        ]
    );
    assert_eq!(
        COLORLESS_RARE_POOL,
        &[
            CardId::SadisticNature,
            CardId::TheBomb,
            CardId::SecretTechnique,
            CardId::Violence,
            CardId::Panache,
            CardId::SecretWeapon,
            CardId::Apotheosis,
            CardId::Mayhem,
            CardId::HandOfGreed,
            CardId::Transmutation,
            CardId::Chrysalis,
            CardId::Magnetism,
            CardId::MasterOfStrategy,
            CardId::Metamorphosis,
            CardId::ThinkingAhead,
        ]
    );
    assert_eq!(
        random_colorless_in_combat_pool(),
        vec![
            CardId::Madness,
            CardId::ThinkingAhead,
            CardId::MindBlast,
            CardId::Metamorphosis,
            CardId::JackOfAllTrades,
            CardId::SwiftStrike,
            CardId::GoodInstincts,
            CardId::MasterOfStrategy,
            CardId::Magnetism,
            CardId::Finesse,
            CardId::Discovery,
            CardId::Chrysalis,
            CardId::Transmutation,
            CardId::Panacea,
            CardId::Purity,
            CardId::Enlightenment,
            CardId::Forethought,
            CardId::FlashOfSteel,
            CardId::HandOfGreed,
            CardId::Mayhem,
            CardId::Apotheosis,
            CardId::SecretWeapon,
            CardId::Panache,
            CardId::Violence,
            CardId::DeepBreath,
            CardId::SecretTechnique,
            CardId::Blind,
            CardId::TheBomb,
            CardId::Impatience,
            CardId::DramaticEntrance,
            CardId::Trip,
            CardId::PanicButton,
            CardId::SadisticNature,
            CardId::DarkShackles,
        ],
        "Java random colorless combat pool filters HEALING cards after HashMap-order colorless pool construction"
    );
    assert_eq!(
        get_curse_pool(),
        &[
            CardId::Regret,
            CardId::Injury,
            CardId::Shame,
            CardId::Parasite,
            CardId::Normality,
            CardId::Doubt,
            CardId::Writhe,
            CardId::Pain,
            CardId::Decay,
            CardId::Clumsy,
        ]
    );
    for (class, healing_cards) in [
        ("Ironclad", &[CardId::Feed, CardId::Reaper][..]),
        ("Silent", &[CardId::Alchemize][..]),
        ("Defect", &[CardId::SelfRepair][..]),
        ("Watcher", &[CardId::LessonLearned, CardId::Wish][..]),
    ] {
        let pool = class_combat_card_pool_for_type(class, None);
        for healing_card in healing_cards {
            assert!(
                !pool.contains(healing_card),
                "Java returnTrulyRandomCardInCombat filters HEALING cards out of {class} combat random pool"
            );
        }
    }
}

#[test]
fn class_combat_random_pool_reverses_each_java_rarity_group() {
    let ironclad = class_combat_card_pool_for_type("Ironclad", None);

    assert_eq!(ironclad[7], CardId::Clothesline);
    assert_eq!(ironclad[12], CardId::ShrugItOff);
    assert_eq!(
        &ironclad_pool_for_type(CardType::Attack)[..6],
        &[
            CardId::SwordBoomerang,
            CardId::PerfectedStrike,
            CardId::HeavyBlade,
            CardId::WildStrike,
            CardId::Headbutt,
            CardId::Clothesline,
        ]
    );
}

#[test]
fn colorless_dramatic_entrance_uses_java_multi_damage_array() {
    let definition = get_card_definition(CardId::DramaticEntrance);
    assert!(
        definition.is_multi_damage,
        "Java DramaticEntrance constructor sets isMultiDamage = true"
    );

    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    state.entities.monsters = vec![first, second];
    crate::content::powers::store::set_powers_for(
        &mut state,
        8,
        vec![Power {
            power_type: PowerId::Vulnerable,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    let actions = resolve_card_play(
        CardId::DramaticEntrance,
        &state,
        &CombatCard::for_scenario(CardId::DramaticEntrance, 910),
        None,
    );
    match &actions[0].action {
        Action::DamageAllEnemies { damages, .. } => assert_eq!(
            damages.as_slice(),
            &[8, 12],
            "Java DramaticEntrance passes this.multiDamage, preserving target-specific modifiers"
        ),
        other => panic!("Dramatic Entrance should emit DamageAllEnemiesAction, got {other:?}"),
    }

    let mut upgraded = CombatCard::for_scenario(CardId::DramaticEntrance, 911);
    upgraded.upgrades = 1;
    let upgraded_actions = resolve_card_play(CardId::DramaticEntrance, &state, &upgraded, None);
    match &upgraded_actions[0].action {
        Action::DamageAllEnemies { damages, .. } => {
            assert_eq!(damages.as_slice(), &[12, 18]);
        }
        other => panic!("Dramatic Entrance+ should emit DamageAllEnemiesAction, got {other:?}"),
    }
}

#[test]
fn silent_direct_attack_batch_runtime_actions_match_java_use_methods() {
    fn assert_damage(action: &Action, target: usize, amount: i32) {
        match action {
            Action::Damage(info) => {
                assert_eq!(info.target, target);
                assert_eq!(info.output, amount);
                assert_eq!(info.damage_type, DamageType::Normal);
            }
            other => panic!("expected DamageAction, got {other:?}"),
        }
    }

    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::JawWorm);
    second.id = 8;
    state.entities.monsters = vec![first, second];

    let backstab = resolve_card_play(
        CardId::Backstab,
        &state,
        &CombatCard::for_scenario(CardId::Backstab, 910),
        Some(7),
    );
    assert_damage(&backstab[0].action, 7, 11);

    let riddle = resolve_card_play(
        CardId::RiddleWithHoles,
        &state,
        &CombatCard::for_scenario(CardId::RiddleWithHoles, 911),
        Some(7),
    );
    assert_eq!(riddle.len(), 5);
    for action in riddle {
        assert_damage(&action.action, 7, 3);
    }

    let die = resolve_card_play(
        CardId::DieDieDie,
        &state,
        &CombatCard::for_scenario(CardId::DieDieDie, 912),
        None,
    );
    assert_eq!(
        die[0].action,
        Action::DamageAllEnemies {
            source: 0,
            damages: smallvec::smallvec![13, 13],
            damage_type: DamageType::Normal,
            is_modified: true,
        }
    );
}

#[test]
fn finisher_reads_attack_count_when_queued_action_executes() {
    let state = crate::test_support::blank_test_combat();
    let finisher = resolve_card_play(
        CardId::Finisher,
        &state,
        &CombatCard::for_scenario(CardId::Finisher, 913),
        Some(7),
    );
    let Action::DamagePerAttackPlayed(info) = finisher[0].action.clone() else {
        panic!("Finisher should emit Java DamagePerAttackPlayedAction");
    };
    assert_eq!(info.target, 7);
    assert_eq!(info.output, 6);

    let mut runtime_state = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 7;
    monster.current_hp = 50;
    runtime_state.entities.monsters = vec![monster];
    for _ in 0..4 {
        runtime_state.turn.increment_attacks_played();
    }
    crate::engine::action_handlers::execute_action(
        Action::DamagePerAttackPlayed(info.clone()),
        &mut runtime_state,
    );
    let queued = [
        runtime_state.pop_next_action(),
        runtime_state.pop_next_action(),
        runtime_state.pop_next_action(),
        runtime_state.pop_next_action(),
    ];
    for action in &queued[..3] {
        match action {
            Some(Action::Damage(damage)) => {
                assert_eq!(damage.target, 7);
                assert_eq!(damage.output, 6);
            }
            other => panic!("Finisher should queue ordinary DamageAction, got {other:?}"),
        }
    }
    assert_eq!(queued[3], None);

    let mut dead_target_state = crate::test_support::blank_test_combat();
    let mut dead_monster = crate::test_support::test_monster(EnemyId::JawWorm);
    dead_monster.id = 7;
    dead_monster.current_hp = 0;
    dead_target_state.entities.monsters = vec![dead_monster];
    dead_target_state.turn.increment_attacks_played();
    dead_target_state.turn.increment_attacks_played();
    crate::engine::action_handlers::execute_action(
        Action::DamagePerAttackPlayed(info),
        &mut dead_target_state,
    );
    assert_eq!(
        dead_target_state.pop_next_action(),
        None,
        "Java DamagePerAttackPlayedAction does nothing unless target.currentHealth > 0"
    );
}

#[test]
fn silent_execution_time_action_cards_match_java_actions() {
    let state = crate::test_support::blank_test_combat();

    let heel = resolve_card_play(
        CardId::HeelHook,
        &state,
        &CombatCard::for_scenario(CardId::HeelHook, 914),
        Some(7),
    );
    let Action::HeelHook(heel_info) = heel[0].action.clone() else {
        panic!("Heel Hook should emit Java HeelHookAction");
    };
    assert_eq!(heel_info.target, 7);
    assert_eq!(heel_info.output, 5);

    let mut weak_state = crate::test_support::blank_test_combat();
    let mut weak_monster = crate::test_support::test_monster(EnemyId::JawWorm);
    weak_monster.id = 7;
    weak_state.entities.monsters = vec![weak_monster];
    weak_state.entities.power_db.insert(
        7,
        vec![Power {
            power_type: PowerId::Weak,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::HeelHook(heel_info.clone()),
        &mut weak_state,
    );
    assert!(matches!(
        weak_state.pop_next_action(),
        Some(Action::Damage(_))
    ));
    assert_eq!(
        weak_state.pop_next_action(),
        Some(Action::GainEnergy { amount: 1 }),
        "Java HeelHookAction addToTop order makes damage execute before the energy follow-up"
    );
    assert_eq!(weak_state.pop_next_action(), Some(Action::DrawCards(1)));

    let mut no_weak_state = crate::test_support::blank_test_combat();
    no_weak_state.entities.monsters = weak_state.entities.monsters.clone();
    crate::engine::action_handlers::execute_action(Action::HeelHook(heel_info), &mut no_weak_state);
    assert!(matches!(
        no_weak_state.pop_next_action(),
        Some(Action::Damage(_))
    ));
    assert_eq!(no_weak_state.pop_next_action(), None);

    let flechettes = resolve_card_play(
        CardId::Flechettes,
        &state,
        &CombatCard::for_scenario(CardId::Flechettes, 915),
        Some(7),
    );
    let Action::Flechettes(flechettes_info) = flechettes[0].action.clone() else {
        panic!("Flechettes should emit Java FlechetteAction");
    };
    let mut flechettes_state = crate::test_support::blank_test_combat();
    flechettes_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::DefendG, 916),
        CombatCard::for_scenario(CardId::Prepared, 917),
        CombatCard::for_scenario(CardId::StrikeG, 918),
    ];
    crate::engine::action_handlers::execute_action(
        Action::Flechettes(flechettes_info),
        &mut flechettes_state,
    );
    assert!(matches!(
        flechettes_state.pop_next_action(),
        Some(Action::Damage(_))
    ));
    assert!(matches!(
        flechettes_state.pop_next_action(),
        Some(Action::Damage(_))
    ));
    assert_eq!(flechettes_state.pop_next_action(), None);

    let expertise = resolve_card_play(
        CardId::Expertise,
        &state,
        &CombatCard::for_scenario(CardId::Expertise, 919),
        None,
    );
    assert_eq!(
        expertise[0].action,
        Action::ExpertiseDraw {
            target_hand_size: 6,
        }
    );
    let mut expertise_state = crate::test_support::blank_test_combat();
    expertise_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::StrikeG, 920),
        CombatCard::for_scenario(CardId::DefendG, 921),
    ];
    crate::engine::action_handlers::execute_action(
        Action::ExpertiseDraw {
            target_hand_size: 6,
        },
        &mut expertise_state,
    );
    assert_eq!(
        expertise_state.pop_next_action(),
        Some(Action::DrawCards(4))
    );

    let escape = resolve_card_play(
        CardId::EscapePlan,
        &state,
        &CombatCard::for_scenario(CardId::EscapePlan, 923),
        None,
    );
    assert_eq!(
        escape[0].action,
        Action::DrawCardsWithHistory {
            amount: 1,
            clear_history: true,
        }
    );
    assert_eq!(
        escape[1].action,
        Action::EscapePlanBlockIfSkill { block: 3 }
    );
    let mut escape_state = crate::test_support::blank_test_combat();
    escape_state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Prepared, 924)]).into();
    escape_state.runtime.last_drawn_cards = vec![DrawnCardRecord {
        card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(999),
        card_id: CardId::StrikeG,
    }];
    crate::engine::action_handlers::execute_action(escape[0].action.clone(), &mut escape_state);
    assert_eq!(
        escape_state.runtime.last_drawn_cards,
        vec![DrawnCardRecord {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(924),
            card_id: CardId::Prepared,
        }]
    );
    crate::engine::action_handlers::execute_action(escape[1].action.clone(), &mut escape_state);
    assert_eq!(
        escape_state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 3,
        })
    );

    let mut split_escape_state = crate::test_support::blank_test_combat();
    split_escape_state.zones.draw_pile.clear();
    split_escape_state.zones.discard_pile =
        vec![CombatCard::for_scenario(CardId::Prepared, 925)].into();
    split_escape_state.runtime.last_drawn_cards = vec![DrawnCardRecord {
        card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(999),
        card_id: CardId::StrikeG,
    }];
    crate::engine::action_handlers::execute_action(
        escape[0].action.clone(),
        &mut split_escape_state,
    );
    assert_eq!(
        split_escape_state.runtime.last_drawn_cards,
        Vec::<DrawnCardRecord>::new()
    );
    assert_eq!(
        split_escape_state.pop_next_action(),
        Some(Action::EmptyDeckShuffle)
    );
    assert_eq!(
        split_escape_state.pop_next_action(),
        Some(Action::DrawCardsWithHistory {
            amount: 1,
            clear_history: false,
        })
    );

    let predator = resolve_card_play(
        CardId::Predator,
        &state,
        &CombatCard::for_scenario(CardId::Predator, 926),
        Some(7),
    );
    match &predator[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.output, 15);
        }
        other => panic!("Predator first action should damage, got {other:?}"),
    }
    assert_eq!(
        predator[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::DrawCardNextTurn,
            amount: 2,
        }
    );

    let post_draw_actions =
        crate::content::powers::resolve_power_on_post_draw(PowerId::DrawCardNextTurn, &state, 0, 2);
    assert_eq!(
        post_draw_actions.as_slice(),
        &[
            Action::DrawCards(2),
            Action::RemovePower {
                target: 0,
                power_id: PowerId::DrawCardNextTurn,
            },
        ]
    );
}

#[test]
fn silent_power_cards_match_java_power_hooks() {
    let state = crate::test_support::blank_test_combat();

    let accuracy = resolve_card_play(
        CardId::Accuracy,
        &state,
        &CombatCard::for_scenario(CardId::Accuracy, 928),
        None,
    );
    assert_eq!(
        accuracy[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::AccuracyPower,
            amount: 4,
        }
    );

    let mut accuracy_state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    accuracy_state.entities.monsters = vec![target];
    accuracy_state.entities.power_db.insert(
        0,
        vec![Power {
            power_type: PowerId::AccuracyPower,
            instance_id: None,
            amount: 4,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let shiv_damage = evaluate_card_for_play(
        &CombatCard::for_scenario(CardId::Shiv, 929),
        &accuracy_state,
        Some(7),
    );
    assert_eq!(shiv_damage.base_damage_mut, 8);

    let caltrops = resolve_card_play(
        CardId::Caltrops,
        &state,
        &CombatCard::for_scenario(CardId::Caltrops, 930),
        None,
    );
    assert_eq!(
        caltrops[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Thorns,
            amount: 3,
        }
    );

    let infinite = resolve_card_play(
        CardId::InfiniteBlades,
        &state,
        &CombatCard::for_scenario(CardId::InfiniteBlades, 931),
        None,
    );
    assert_eq!(
        infinite[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::InfiniteBladesPower,
            amount: 1,
        }
    );
    let mut live_state = crate::test_support::blank_test_combat();
    let mut live_monster = crate::test_support::test_monster(EnemyId::JawWorm);
    live_monster.id = 7;
    live_state.entities.monsters = vec![live_monster];
    let start_actions = crate::content::powers::resolve_power_at_turn_start(
        PowerId::InfiniteBladesPower,
        &mut live_state,
        0,
        1,
    );
    assert_eq!(start_actions.len(), 1);
    match &start_actions[0] {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Shiv);
            assert_eq!(*amount, 1);
        }
        other => panic!("Infinite Blades should generate constructed Shiv, got {other:?}"),
    }
}

#[test]
fn bouncing_flask_locks_initial_random_target_when_card_is_used() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    second.slot = 1;
    state.entities.monsters = vec![first, second];
    state.zones.hand = vec![CombatCard::for_scenario(CardId::BouncingFlask, 9320)];

    assert_eq!(state.rng.card_random_rng.counter, 0);
    crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut state)
        .expect("Bouncing Flask should be playable");

    assert_eq!(
        state.rng.card_random_rng.counter, 1,
        "Java BouncingFlask.use chooses the first random monster immediately"
    );
    match state.pop_next_action() {
        Some(Action::BouncingFlask {
            target: Some(7 | 8),
            amount: 3,
            num_times: 3,
        }) => {}
        other => panic!("Bouncing Flask should enqueue a locked first target, got {other:?}"),
    }
}

#[test]
fn bouncing_flask_uses_inclusive_range_for_every_bounce_and_no_rng_without_targets() {
    use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};
    for enemies in 0..=2 {
        for upgrades in [0, 1] {
            let mut state = crate::test_support::blank_test_combat();
            for index in 0..enemies {
                let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
                monster.id = index + 1;
                monster.slot = index as u8;
                state.entities.monsters.push(monster);
            }
            let mut card = CombatCard::for_scenario(CardId::BouncingFlask, 9321);
            card.upgrades = upgrades;
            state.zones.hand = vec![card];
            let trace = begin_sts_rng_trace().unwrap();
            crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut state)
                .expect("Flask can be used without a manually selected monster");
            while let Some(action) = state.pop_next_action() {
                crate::engine::action_handlers::execute_action(action, &mut state);
            }
            let calls = trace.finish().unwrap();
            assert_eq!(
                calls.len(),
                if enemies == 0 {
                    0
                } else {
                    3 + upgrades as usize
                }
            );
            for call in calls {
                assert_eq!(call.owner, Some("floor.card_random"));
                assert!(
                    matches!(call.call, StsRngCall::RandomIntRange { start: 0, end, result }
                    if end == enemies as i32 - 1 && (0..=end).contains(&result))
                );
            }
        }
    }
}

#[test]
fn silent_dynamic_cost_cards_match_java_draw_discard_and_damage_hooks() {
    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    state.entities.monsters = vec![target];

    let eviscerate = resolve_card_play(
        CardId::Eviscerate,
        &state,
        &CombatCard::for_scenario(CardId::Eviscerate, 932),
        Some(7),
    );
    assert_eq!(eviscerate.len(), 3);
    for action in eviscerate {
        match action.action {
            Action::Damage(info) => {
                assert_eq!(info.target, 7);
                assert_eq!(info.output, 7);
            }
            other => panic!("Eviscerate should queue three DamageActions, got {other:?}"),
        }
    }

    let masterful = resolve_card_play(
        CardId::MasterfulStab,
        &state,
        &CombatCard::for_scenario(CardId::MasterfulStab, 933),
        Some(7),
    );
    match &masterful[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.output, 12);
        }
        other => panic!("Masterful Stab should queue DamageAction, got {other:?}"),
    }

    let mut discard_state = crate::test_support::blank_test_combat();
    discard_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::StrikeG, 934),
        CombatCard::for_scenario(CardId::Eviscerate, 935),
    ];
    discard_state.zones.draw_pile =
        (vec![CombatCard::for_scenario(CardId::Eviscerate, 936)]).into();
    discard_state.zones.discard_pile =
        vec![CombatCard::for_scenario(CardId::Eviscerate, 937)].into();
    crate::engine::action_handlers::cards::handle_discard_card(
        crate::runtime::combat::CardLineageId::constructed_scenario(934),
        &mut discard_state,
    );
    assert_eq!(discard_state.turn.counters.cards_discarded_this_turn, 1);
    assert_eq!(discard_state.zones.hand[0].cost_for_turn_java(), 2);
    assert_eq!(discard_state.zones.draw_pile[0].cost_for_turn_java(), 2);
    assert_eq!(discard_state.zones.discard_pile[0].cost_for_turn_java(), 2);

    let mut end_turn_discard_state = crate::test_support::blank_test_combat();
    end_turn_discard_state.zones.hand = vec![CombatCard::for_scenario(CardId::StrikeG, 938)];
    end_turn_discard_state.zones.draw_pile =
        (vec![CombatCard::for_scenario(CardId::Eviscerate, 939)]).into();
    crate::engine::action_handlers::cards::handle_discard_from_hand(
        1,
        false,
        true,
        &mut end_turn_discard_state,
    );
    assert_eq!(
        end_turn_discard_state.zones.draw_pile[0].cost_for_turn_java(),
        3,
        "Java incrementDiscard(endTurn=true) increments count but does not call updateCardsOnDiscard"
    );

    let mut draw_state = crate::test_support::blank_test_combat();
    draw_state.turn.increment_cards_discarded();
    draw_state.turn.increment_cards_discarded();
    draw_state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Eviscerate, 940)]).into();
    crate::engine::action_handlers::execute_action(Action::DrawCards(1), &mut draw_state);
    assert_eq!(draw_state.zones.hand[0].cost_for_turn_java(), 1);

    crate::content::cards::hooks::at_turn_start_in_hand(&mut draw_state);
    assert_eq!(
        draw_state.zones.hand[0].cost_for_turn_java(),
        3,
        "Java Eviscerate.atTurnStart resetAttributes clears temporary discard reductions"
    );

    let mut damage_state = crate::test_support::blank_test_combat();
    let mut attacker = crate::test_support::test_monster(EnemyId::JawWorm);
    attacker.id = 7;
    damage_state.entities.monsters = vec![attacker];
    damage_state.entities.player.current_hp = 50;
    damage_state.zones.hand = vec![CombatCard::for_scenario(CardId::MasterfulStab, 941)];
    damage_state.zones.draw_pile =
        (vec![CombatCard::for_scenario(CardId::MasterfulStab, 942)]).into();
    damage_state.zones.discard_pile =
        vec![CombatCard::for_scenario(CardId::MasterfulStab, 943)].into();
    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: 7,
            target: 0,
            base: 1,
            output: 1,
            damage_type: DamageType::Normal,
            is_modified: false,
        }),
        &mut damage_state,
    );
    assert_eq!(
        damage_state.zones.hand[0].combat_cost_without_turn_override_java(),
        1
    );
    assert_eq!(
        damage_state.zones.draw_pile[0].combat_cost_without_turn_override_java(),
        1
    );
    assert_eq!(
        damage_state.zones.discard_pile[0].combat_cost_without_turn_override_java(),
        1
    );
}

#[test]
fn silent_target_control_cards_match_java_sources_and_power_hooks() {
    let blur = get_card_definition(CardId::Blur);
    assert_eq!(blur.name, "Blur");
    assert_eq!(blur.card_type, CardType::Skill);
    assert_eq!(blur.rarity, CardRarity::Uncommon);
    assert_eq!(blur.cost, 1);
    assert_eq!(blur.base_block, 5);
    assert_eq!(blur.upgrade_block, 3);
    assert_eq!(java_id(CardId::Blur), "Blur");

    let choke = get_card_definition(CardId::Choke);
    assert_eq!(choke.name, "Choke");
    assert_eq!(choke.card_type, CardType::Attack);
    assert_eq!(choke.rarity, CardRarity::Uncommon);
    assert_eq!(choke.cost, 2);
    assert_eq!(choke.base_damage, 12);
    assert_eq!(choke.base_magic, 3);
    assert_eq!(choke.upgrade_magic, 2);
    assert_eq!(java_id(CardId::Choke), "Choke");

    let crippling = get_card_definition(CardId::CripplingPoison);
    assert_eq!(crippling.name, "Crippling Poison");
    assert_eq!(crippling.card_type, CardType::Skill);
    assert_eq!(crippling.rarity, CardRarity::Uncommon);
    assert_eq!(crippling.cost, 2);
    assert_eq!(crippling.base_magic, 4);
    assert_eq!(crippling.upgrade_magic, 3);
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::CripplingPoison,
        950
    )));
    assert_eq!(java_id(CardId::CripplingPoison), "Crippling Poison");

    let leg_sweep = get_card_definition(CardId::LegSweep);
    assert_eq!(leg_sweep.name, "Leg Sweep");
    assert_eq!(leg_sweep.card_type, CardType::Skill);
    assert_eq!(leg_sweep.rarity, CardRarity::Uncommon);
    assert_eq!(leg_sweep.cost, 2);
    assert_eq!(leg_sweep.base_block, 11);
    assert_eq!(leg_sweep.base_magic, 2);
    assert_eq!(leg_sweep.upgrade_block, 3);
    assert_eq!(leg_sweep.upgrade_magic, 1);
    assert_eq!(java_id(CardId::LegSweep), "Leg Sweep");

    let terror = get_card_definition(CardId::Terror);
    assert_eq!(terror.name, "Terror");
    assert_eq!(terror.card_type, CardType::Skill);
    assert_eq!(terror.rarity, CardRarity::Uncommon);
    assert_eq!(terror.cost, 1);
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Terror,
        951
    )));
    let mut terror_plus = CombatCard::for_scenario(CardId::Terror, 952);
    terror_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&terror_plus), Some(0));
    assert_eq!(java_id(CardId::Terror), "Terror");

    assert_eq!(build_java_id_map().get("Blur"), Some(&CardId::Blur));
    assert_eq!(build_java_id_map().get("Choke"), Some(&CardId::Choke));
    assert_eq!(
        build_java_id_map().get("Crippling Poison"),
        Some(&CardId::CripplingPoison)
    );
    assert_eq!(
        build_java_id_map().get("Leg Sweep"),
        Some(&CardId::LegSweep)
    );
    assert_eq!(build_java_id_map().get("Terror"), Some(&CardId::Terror));

    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    state.entities.monsters = vec![first, second];

    let blur_actions = resolve_card_play(
        CardId::Blur,
        &state,
        &CombatCard::for_scenario(CardId::Blur, 953),
        None,
    );
    assert_eq!(blur_actions.len(), 2);
    assert!(matches!(
        blur_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 5
        }
    ));
    assert!(matches!(
        blur_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Blur,
            amount: 1
        }
    ));

    let choke_actions = resolve_card_play(
        CardId::Choke,
        &state,
        &CombatCard::for_scenario(CardId::Choke, 954),
        Some(7),
    );
    assert_eq!(choke_actions.len(), 2);
    match &choke_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.output, 12);
        }
        other => panic!("Choke should first queue DamageAction, got {other:?}"),
    }
    assert!(matches!(
        choke_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Choked,
            amount: 3
        }
    ));

    let choked_actions = crate::content::powers::resolve_power_on_card_played(
        PowerId::Choked,
        &state,
        7,
        &CombatCard::for_scenario(CardId::DefendG, 955),
        3,
    );
    assert_eq!(
        choked_actions[0],
        Action::LoseHp {
            target: 7,
            amount: 3,
            triggers_rupture: false,
        }
    );
    assert_eq!(
        crate::content::powers::resolve_power_at_turn_start(PowerId::Choked, &mut state, 7, 3)[0],
        Action::RemovePower {
            target: 7,
            power_id: PowerId::Choked
        }
    );

    let mut leg_plus = CombatCard::for_scenario(CardId::LegSweep, 956);
    leg_plus.upgrades = 1;
    let leg_actions = resolve_card_play(CardId::LegSweep, &state, &leg_plus, Some(7));
    assert_eq!(leg_actions.len(), 2);
    assert!(matches!(
        leg_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Weak,
            amount: 3
        }
    ));
    assert!(matches!(
        leg_actions[1].action,
        Action::GainBlock {
            target: 0,
            amount: 14
        }
    ));

    let terror_actions = resolve_card_play(
        CardId::Terror,
        &state,
        &CombatCard::for_scenario(CardId::Terror, 957),
        Some(8),
    );
    assert_eq!(
        terror_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 8,
            power_id: PowerId::Vulnerable,
            amount: 99,
        }
    );

    let mut cripple_state = state.clone();
    cripple_state.entities.monsters[1].is_dying = true;
    let cripple_actions = resolve_card_play(
        CardId::CripplingPoison,
        &cripple_state,
        &CombatCard::for_scenario(CardId::CripplingPoison, 958),
        None,
    );
    assert_eq!(
        cripple_actions
            .iter()
            .map(|info| &info.action)
            .collect::<Vec<_>>(),
        vec![
            &Action::ApplyPower {
                source: 0,
                target: 7,
                power_id: PowerId::Poison,
                amount: 4,
            },
            &Action::ApplyPower {
                source: 0,
                target: 7,
                power_id: PowerId::Weak,
                amount: 2,
            },
        ]
    );

    assert_eq!(
        crate::content::powers::resolve_power_at_end_of_round(PowerId::Blur, &state, 0, 1, false)
            [0],
        Action::ReducePower {
            target: 0,
            power_id: PowerId::Blur,
            amount: 1,
        }
    );
}

#[test]
fn silent_special_attack_cards_match_java_draw_and_mutation_hooks() {
    let endless = get_card_definition(CardId::EndlessAgony);
    assert_eq!(endless.name, "Endless Agony");
    assert_eq!(endless.card_type, CardType::Attack);
    assert_eq!(endless.rarity, CardRarity::Uncommon);
    assert_eq!(endless.cost, 0);
    assert_eq!(endless.base_damage, 4);
    assert_eq!(endless.upgrade_damage, 2);
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::EndlessAgony,
        959
    )));
    assert_eq!(java_id(CardId::EndlessAgony), "Endless Agony");

    let glass = get_card_definition(CardId::GlassKnife);
    assert_eq!(glass.name, "Glass Knife");
    assert_eq!(glass.card_type, CardType::Attack);
    assert_eq!(glass.rarity, CardRarity::Rare);
    assert_eq!(glass.cost, 1);
    assert_eq!(glass.base_damage, 8);
    assert_eq!(glass.upgrade_damage, 4);
    assert_eq!(java_id(CardId::GlassKnife), "Glass Knife");

    let finale = get_card_definition(CardId::GrandFinale);
    assert_eq!(finale.name, "Grand Finale");
    assert_eq!(finale.card_type, CardType::Attack);
    assert_eq!(finale.rarity, CardRarity::Rare);
    assert_eq!(finale.cost, 0);
    assert_eq!(finale.base_damage, 50);
    assert_eq!(finale.upgrade_damage, 10);
    assert!(finale.is_multi_damage);
    assert_eq!(java_id(CardId::GrandFinale), "Grand Finale");
    let skewer = get_card_definition(CardId::Skewer);
    assert_eq!(skewer.name, "Skewer");
    assert_eq!(skewer.card_type, CardType::Attack);
    assert_eq!(skewer.rarity, CardRarity::Uncommon);
    assert_eq!(skewer.cost, -1);
    assert_eq!(skewer.base_damage, 7);
    assert_eq!(skewer.upgrade_damage, 3);
    assert_eq!(skewer.target, CardTarget::Enemy);
    assert_eq!(java_id(CardId::Skewer), "Skewer");

    assert_eq!(
        build_java_id_map().get("Endless Agony"),
        Some(&CardId::EndlessAgony)
    );
    assert_eq!(
        build_java_id_map().get("Glass Knife"),
        Some(&CardId::GlassKnife)
    );
    assert_eq!(
        build_java_id_map().get("Grand Finale"),
        Some(&CardId::GrandFinale)
    );
    assert_eq!(build_java_id_map().get("Skewer"), Some(&CardId::Skewer));

    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    state.entities.monsters = vec![first, second];

    let endless_actions = resolve_card_play(
        CardId::EndlessAgony,
        &state,
        &CombatCard::for_scenario(CardId::EndlessAgony, 960),
        Some(7),
    );
    match &endless_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.output, 4);
        }
        other => panic!("Endless Agony should queue DamageAction, got {other:?}"),
    }

    let glass_actions = resolve_card_play(
        CardId::GlassKnife,
        &state,
        &CombatCard::for_scenario(CardId::GlassKnife, 961),
        Some(7),
    );
    assert_eq!(glass_actions.len(), 3);
    for action in glass_actions.iter().take(2) {
        match &action.action {
            Action::Damage(info) => {
                assert_eq!(info.target, 7);
                assert_eq!(info.output, 8);
            }
            other => panic!("Glass Knife should queue two DamageActions, got {other:?}"),
        }
    }
    assert_eq!(
        glass_actions[2].action,
        Action::ModifyCardDamage {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(961),
            amount: -2,
        }
    );

    let finale_actions = resolve_card_play(
        CardId::GrandFinale,
        &state,
        &CombatCard::for_scenario(CardId::GrandFinale, 962),
        None,
    );
    assert_eq!(
        finale_actions[0].action,
        Action::DamageAllEnemies {
            source: 0,
            damages: smallvec::smallvec![50, 50],
            damage_type: DamageType::Normal,
            is_modified: true,
        }
    );

    let mut finale_blocked = state.clone();
    finale_blocked.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::StrikeG, 963)]).into();
    assert!(
        can_play_card(
            &CombatCard::for_scenario(CardId::GrandFinale, 964),
            &finale_blocked
        )
        .is_err(),
        "Java Grand Finale.canUse requires an empty draw pile"
    );
    let mut finale_allowed = state.clone();
    finale_allowed.zones.draw_pile.clear();
    assert!(can_play_card(
        &CombatCard::for_scenario(CardId::GrandFinale, 965),
        &finale_allowed
    )
    .is_ok());

    let mut draw_state = crate::test_support::blank_test_combat();
    let mut drawn = CombatCard::for_scenario(CardId::EndlessAgony, 966);
    drawn.upgrades = 1;
    draw_state.zones.draw_pile = (vec![drawn]).into();
    crate::engine::action_handlers::execute_action(Action::DrawCards(1), &mut draw_state);
    assert_eq!(draw_state.zones.hand.len(), 1);
    assert_eq!(draw_state.zones.hand[0].id, CardId::EndlessAgony);
    let trigger = draw_state
        .pop_next_action()
        .expect("Endless Agony.triggerWhenDrawn should queue MakeTempCardInHandAction");
    match trigger {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::EndlessAgony);
            assert_eq!(original.upgrades, 1);
            assert_eq!(amount, 1);
            crate::engine::action_handlers::execute_action(
                Action::MakeConstructedCopyInHand { original, amount },
                &mut draw_state,
            );
        }
        other => panic!("Endless Agony draw hook should make a hand copy, got {other:?}"),
    }
    assert_eq!(draw_state.zones.hand.len(), 2);
    assert_eq!(draw_state.zones.hand[1].id, CardId::EndlessAgony);
    assert_eq!(draw_state.zones.hand[1].upgrades, 1);

    let mut knife_state = crate::test_support::blank_test_combat();
    knife_state.zones.limbo = vec![CombatCard::for_scenario(CardId::GlassKnife, 967)];
    crate::engine::action_handlers::execute_action(
        Action::ModifyCardDamage {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(967),
            amount: -2,
        },
        &mut knife_state,
    );
    assert_eq!(knife_state.zones.limbo[0].base_damage_override, Some(6));
}

#[test]
fn skewer_matches_java_x_cost_single_target_action() {
    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    state.entities.monsters = vec![target];

    let mut skewer_card = CombatCard::for_scenario(CardId::Skewer, 970);
    skewer_card.energy_on_use = 2;
    let actions = resolve_card_play(CardId::Skewer, &state, &skewer_card, Some(7));
    assert_eq!(actions.len(), 1);
    match &actions[0].action {
        Action::Skewer {
            target,
            damage_info,
            free_to_play_once,
            energy_on_use,
        } => {
            assert_eq!(*target, 7);
            assert_eq!(damage_info.target, 7);
            assert_eq!(damage_info.output, 7);
            assert!(!free_to_play_once);
            assert_eq!(*energy_on_use, 2);
        }
        other => panic!("Skewer should emit SkewerAction equivalent, got {other:?}"),
    }

    let mut energy_state = crate::test_support::blank_test_combat();
    energy_state.turn.energy = 5;
    crate::engine::action_handlers::execute_action(
        Action::Skewer {
            target: 7,
            damage_info: DamageInfo {
                source: 0,
                target: 7,
                base: 7,
                output: 7,
                damage_type: DamageType::Normal,
                is_modified: true,
            },
            free_to_play_once: false,
            energy_on_use: 2,
        },
        &mut energy_state,
    );
    assert_eq!(
        energy_state.turn.energy, 0,
        "Java SkewerAction spends EnergyPanel.totalCount, not energyOnUse"
    );
    for _ in 0..2 {
        match energy_state.pop_next_action() {
            Some(Action::Damage(info)) => {
                assert_eq!(info.target, 7);
                assert_eq!(info.output, 7);
            }
            other => panic!("SkewerAction should queue fixed target DamageAction, got {other:?}"),
        }
    }
    assert!(energy_state.pop_next_action().is_none());

    let mut chemical_x_state = crate::test_support::blank_test_combat();
    chemical_x_state.turn.energy = 0;
    chemical_x_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::ChemicalX,
        ));
    crate::engine::action_handlers::execute_action(
        Action::Skewer {
            target: 7,
            damage_info: DamageInfo {
                source: 0,
                target: 7,
                base: 7,
                output: 7,
                damage_type: DamageType::Normal,
                is_modified: true,
            },
            free_to_play_once: false,
            energy_on_use: -1,
        },
        &mut chemical_x_state,
    );
    assert_eq!(
        chemical_x_state.action_queue_len(),
        2,
        "Chemical X adds two Skewer hits even when current energy is zero"
    );
}

#[test]
fn silent_x_cost_power_cards_match_java_actions() {
    let doppelganger = get_card_definition(CardId::Doppelganger);
    assert_eq!(doppelganger.name, "Doppelganger");
    assert_eq!(doppelganger.card_type, CardType::Skill);
    assert_eq!(doppelganger.rarity, CardRarity::Rare);
    assert_eq!(doppelganger.cost, -1);
    assert_eq!(doppelganger.target, CardTarget::SelfTarget);
    assert!(doppelganger.exhaust);
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Doppelganger,
        980
    )));
    assert_eq!(java_id(CardId::Doppelganger), "Doppelganger");

    let malaise = get_card_definition(CardId::Malaise);
    assert_eq!(malaise.name, "Malaise");
    assert_eq!(malaise.card_type, CardType::Skill);
    assert_eq!(malaise.rarity, CardRarity::Rare);
    assert_eq!(malaise.cost, -1);
    assert_eq!(malaise.target, CardTarget::Enemy);
    assert!(malaise.exhaust);
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Malaise,
        981
    )));
    assert_eq!(java_id(CardId::Malaise), "Malaise");

    assert_eq!(
        build_java_id_map().get("Doppelganger"),
        Some(&CardId::Doppelganger)
    );
    assert_eq!(build_java_id_map().get("Malaise"), Some(&CardId::Malaise));

    let state = crate::test_support::blank_test_combat();
    let mut doppelganger_card = CombatCard::for_scenario(CardId::Doppelganger, 982);
    doppelganger_card.upgrades = 1;
    doppelganger_card.energy_on_use = 2;
    let doppelganger_actions =
        resolve_card_play(CardId::Doppelganger, &state, &doppelganger_card, None);
    assert_eq!(doppelganger_actions.len(), 1);
    assert_eq!(
        doppelganger_actions[0].action,
        Action::Doppelganger {
            upgraded: true,
            free_to_play_once: false,
            energy_on_use: 2,
        }
    );

    let mut malaise_card = CombatCard::for_scenario(CardId::Malaise, 983);
    malaise_card.upgrades = 1;
    malaise_card.free_to_play_once = true;
    malaise_card.energy_on_use = 2;
    let malaise_actions = resolve_card_play(CardId::Malaise, &state, &malaise_card, Some(7));
    assert_eq!(malaise_actions.len(), 1);
    assert_eq!(
        malaise_actions[0].action,
        Action::Malaise {
            target: 7,
            upgraded: true,
            free_to_play_once: true,
            energy_on_use: 2,
        }
    );

    let mut doppelganger_state = crate::test_support::blank_test_combat();
    doppelganger_state.turn.energy = 5;
    crate::engine::action_handlers::execute_action(
        Action::Doppelganger {
            upgraded: true,
            free_to_play_once: false,
            energy_on_use: 2,
        },
        &mut doppelganger_state,
    );
    assert_eq!(
        doppelganger_state.turn.energy, 0,
        "Java DoppelgangerAction spends current EnergyPanel.totalCount"
    );
    assert_eq!(
        doppelganger_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Energized,
            amount: 3,
        })
    );
    assert_eq!(
        doppelganger_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::DrawCardNextTurn,
            amount: 3,
        })
    );
    assert!(doppelganger_state.pop_next_action().is_none());

    let mut free_doppelganger_state = crate::test_support::blank_test_combat();
    free_doppelganger_state.turn.energy = 4;
    crate::engine::action_handlers::execute_action(
        Action::Doppelganger {
            upgraded: true,
            free_to_play_once: true,
            energy_on_use: -1,
        },
        &mut free_doppelganger_state,
    );
    assert_eq!(
        free_doppelganger_state.turn.energy, 4,
        "free-to-play Doppelganger uses current energy for X but does not spend it"
    );
    assert_eq!(
        free_doppelganger_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Energized,
            amount: 5,
        })
    );
    assert_eq!(
        free_doppelganger_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::DrawCardNextTurn,
            amount: 5,
        })
    );

    let mut malaise_state = crate::test_support::blank_test_combat();
    malaise_state.turn.energy = 5;
    crate::engine::action_handlers::execute_action(
        Action::Malaise {
            target: 7,
            upgraded: false,
            free_to_play_once: false,
            energy_on_use: 2,
        },
        &mut malaise_state,
    );
    assert_eq!(
        malaise_state.turn.energy, 0,
        "Java MalaiseAction spends current EnergyPanel.totalCount"
    );
    assert_eq!(
        malaise_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Strength,
            amount: -2,
        })
    );
    assert_eq!(
        malaise_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Weak,
            amount: 2,
        })
    );
    assert!(malaise_state.pop_next_action().is_none());

    let mut free_malaise_state = crate::test_support::blank_test_combat();
    free_malaise_state.turn.energy = 3;
    crate::engine::action_handlers::execute_action(
        Action::Malaise {
            target: 7,
            upgraded: true,
            free_to_play_once: true,
            energy_on_use: 2,
        },
        &mut free_malaise_state,
    );
    assert_eq!(
        free_malaise_state.turn.energy, 3,
        "free-to-play Malaise keeps energy but still uses energy_on_use for X"
    );
    assert_eq!(
        free_malaise_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Strength,
            amount: -3,
        })
    );
    assert_eq!(
        free_malaise_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Weak,
            amount: 3,
        })
    );

    let mut chemical_x_state = crate::test_support::blank_test_combat();
    chemical_x_state.turn.energy = 0;
    chemical_x_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::ChemicalX,
        ));
    crate::engine::action_handlers::execute_action(
        Action::Malaise {
            target: 7,
            upgraded: true,
            free_to_play_once: false,
            energy_on_use: -1,
        },
        &mut chemical_x_state,
    );
    assert_eq!(
        chemical_x_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Strength,
            amount: -3,
        })
    );
    assert_eq!(
        chemical_x_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Weak,
            amount: 3,
        })
    );
}

#[test]
fn wraith_form_matches_java_intangible_and_dexterity_loss_power() {
    let wraith = get_card_definition(CardId::WraithForm);
    assert_eq!(wraith.name, "Wraith Form");
    assert_eq!(wraith.card_type, CardType::Power);
    assert_eq!(wraith.rarity, CardRarity::Rare);
    assert_eq!(wraith.cost, 3);
    assert_eq!(wraith.base_magic, 2);
    assert_eq!(wraith.upgrade_magic, 1);
    assert_eq!(wraith.target, CardTarget::SelfTarget);
    assert_eq!(java_id(CardId::WraithForm), "Wraith Form v2");
    assert_eq!(
        build_java_id_map().get("Wraith Form v2"),
        Some(&CardId::WraithForm)
    );

    let mut state = crate::test_support::blank_test_combat();
    let mut wraith_plus = CombatCard::for_scenario(CardId::WraithForm, 990);
    wraith_plus.upgrades = 1;
    let actions = resolve_card_play(CardId::WraithForm, &state, &wraith_plus, None);
    assert_eq!(actions.len(), 2);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::IntangiblePlayer,
            amount: 3,
        }
    );
    assert_eq!(
        actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::WraithForm,
            amount: -1,
        }
    );

    assert!(
        crate::content::powers::allows_negative_amount(PowerId::WraithForm),
        "Java WraithFormPower stacks by adding negative amounts"
    );
    assert!(
        crate::content::powers::is_debuff_application(PowerId::WraithForm, -1),
        "Java WraithFormPower is PowerType.DEBUFF even though it is applied by a card"
    );

    let wraith_power = Power {
        power_type: PowerId::WraithForm,
        instance_id: None,
        amount: -1,
        extra_data: 0,
        payload: crate::runtime::combat::PowerPayload::None,
        just_applied: false,
    };
    let turn_end =
        crate::content::powers::resolve_power_at_end_of_turn(&wraith_power, &mut state, 0);
    assert_eq!(turn_end.len(), 1);
    assert_eq!(
        turn_end[0],
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Dexterity,
            amount: -1,
        }
    );
}

#[test]
fn phantasmal_killer_matches_java_delayed_double_damage_power() {
    let phantasmal = get_card_definition(CardId::PhantasmalKiller);
    assert_eq!(phantasmal.name, "Phantasmal Killer");
    assert_eq!(phantasmal.card_type, CardType::Skill);
    assert_eq!(phantasmal.rarity, CardRarity::Rare);
    assert_eq!(phantasmal.cost, 1);
    assert_eq!(phantasmal.target, CardTarget::SelfTarget);
    assert_eq!(java_id(CardId::PhantasmalKiller), "Phantasmal Killer");
    assert_eq!(
        build_java_id_map().get("Phantasmal Killer"),
        Some(&CardId::PhantasmalKiller)
    );
    let mut phantasmal_plus = CombatCard::for_scenario(CardId::PhantasmalKiller, 995);
    phantasmal_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&phantasmal_plus), Some(0));

    let mut state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(CardId::PhantasmalKiller, &state, &phantasmal_plus, None);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Phantasmal,
            amount: 1,
        }
    );

    let turn_start =
        crate::content::powers::resolve_power_at_turn_start(PowerId::Phantasmal, &mut state, 0, 2);
    assert_eq!(
        turn_start[0],
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::DoubleDamage,
            amount: 1,
        }
    );
    assert_eq!(
        turn_start[1],
        Action::ReducePower {
            target: 0,
            power_id: PowerId::Phantasmal,
            amount: 1,
        }
    );

    let double_damage_round_end = crate::content::powers::resolve_power_at_end_of_round(
        PowerId::DoubleDamage,
        &state,
        0,
        1,
        false,
    );
    assert_eq!(
        double_damage_round_end[0],
        Action::ReducePower {
            target: 0,
            power_id: PowerId::DoubleDamage,
            amount: 1,
        }
    );

    let mut damage_state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    damage_state.entities.monsters = vec![target];
    damage_state.entities.power_db.insert(
        0,
        vec![Power {
            power_type: PowerId::DoubleDamage,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let strike_actions = resolve_card_play(
        CardId::StrikeG,
        &damage_state,
        &CombatCard::for_scenario(CardId::StrikeG, 996),
        Some(7),
    );
    match &strike_actions[0].action {
        Action::Damage(info) => assert_eq!(info.output, 12),
        other => panic!("DoubleDamagePower should affect normal attack damage, got {other:?}"),
    }
}

#[test]
fn envenom_matches_java_owner_on_attack_poison_hook() {
    let envenom = get_card_definition(CardId::Envenom);
    assert_eq!(envenom.name, "Envenom");
    assert_eq!(envenom.card_type, CardType::Power);
    assert_eq!(envenom.rarity, CardRarity::Rare);
    assert_eq!(envenom.cost, 2);
    assert_eq!(envenom.target, CardTarget::SelfTarget);
    assert_eq!(java_id(CardId::Envenom), "Envenom");
    assert_eq!(build_java_id_map().get("Envenom"), Some(&CardId::Envenom));
    let mut envenom_plus = CombatCard::for_scenario(CardId::Envenom, 997);
    envenom_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&envenom_plus), Some(1));

    let actions = resolve_card_play(
        CardId::Envenom,
        &crate::test_support::blank_test_combat(),
        &envenom_plus,
        None,
    );
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Envenom,
            amount: 1,
        }
    );

    let mut poison_state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    target.current_hp = 50;
    target.max_hp = 50;
    poison_state.entities.monsters = vec![target];
    poison_state.entities.power_db.insert(
        0,
        vec![Power {
            power_type: PowerId::Envenom,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: 0,
            target: 7,
            base: 6,
            output: 6,
            damage_type: DamageType::Normal,
            is_modified: true,
        }),
        &mut poison_state,
    );
    assert_eq!(
        poison_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Poison,
            amount: 1,
        })
    );

    let mut blocked_state = poison_state.clone();
    blocked_state.clear_pending_actions();
    blocked_state.entities.monsters[0].current_hp = 50;
    blocked_state.entities.monsters[0].block = 20;
    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: 0,
            target: 7,
            base: 6,
            output: 6,
            damage_type: DamageType::Normal,
            is_modified: true,
        }),
        &mut blocked_state,
    );
    assert!(
        blocked_state.pop_next_action().is_none(),
        "Java EnvenomPower.onAttack requires post-block damageAmount > 0"
    );

    let mut thorns_state = poison_state.clone();
    thorns_state.clear_pending_actions();
    thorns_state.entities.monsters[0].current_hp = 50;
    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: 0,
            target: 7,
            base: 6,
            output: 6,
            damage_type: DamageType::Thorns,
            is_modified: true,
        }),
        &mut thorns_state,
    );
    assert!(
        thorns_state.pop_next_action().is_none(),
        "Java EnvenomPower.onAttack only triggers for NORMAL damage"
    );
}

#[test]
fn a_thousand_cuts_matches_java_power_and_thorns_damage_hook() {
    let cuts = get_card_definition(CardId::AThousandCuts);
    assert_eq!(cuts.name, "A Thousand Cuts");
    assert_eq!(cuts.card_type, CardType::Power);
    assert_eq!(cuts.rarity, CardRarity::Rare);
    assert_eq!(cuts.cost, 2);
    assert_eq!(cuts.base_magic, 1);
    assert_eq!(cuts.upgrade_magic, 1);
    assert_eq!(cuts.target, CardTarget::SelfTarget);
    assert_eq!(java_id(CardId::AThousandCuts), "A Thousand Cuts");
    assert_eq!(
        build_java_id_map().get("A Thousand Cuts"),
        Some(&CardId::AThousandCuts)
    );

    let state = crate::test_support::blank_test_combat();
    let mut cuts_plus = CombatCard::for_scenario(CardId::AThousandCuts, 998);
    cuts_plus.upgrades = 1;
    let actions = resolve_card_play(CardId::AThousandCuts, &state, &cuts_plus, None);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::ThousandCuts,
            amount: 2,
        }
    );

    let mut hook_state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    hook_state.entities.monsters = vec![first, second];
    let hook_actions = crate::content::powers::resolve_power_on_card_played(
        PowerId::ThousandCuts,
        &hook_state,
        0,
        &CombatCard::for_scenario(CardId::StrikeG, 999),
        2,
    );
    assert_eq!(
        hook_actions[0],
        Action::DamageAllEnemies {
            source: 0,
            damages: smallvec::smallvec![2, 2],
            damage_type: DamageType::Thorns,
            is_modified: false,
        }
    );
}

#[test]
fn bullet_time_matches_java_no_draw_and_hand_cost_override() {
    let bullet_time = get_card_definition(CardId::BulletTime);
    assert_eq!(bullet_time.name, "Bullet Time");
    assert_eq!(bullet_time.card_type, CardType::Skill);
    assert_eq!(bullet_time.rarity, CardRarity::Rare);
    assert_eq!(bullet_time.cost, 3);
    assert_eq!(bullet_time.target, CardTarget::None);
    assert_eq!(java_id(CardId::BulletTime), "Bullet Time");
    assert_eq!(
        build_java_id_map().get("Bullet Time"),
        Some(&CardId::BulletTime)
    );
    let mut bullet_time_plus = CombatCard::for_scenario(CardId::BulletTime, 1000);
    bullet_time_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&bullet_time_plus), Some(2));

    let actions = resolve_card_play(
        CardId::BulletTime,
        &crate::test_support::blank_test_combat(),
        &bullet_time_plus,
        None,
    );
    assert_eq!(actions.len(), 2);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::NoDraw,
            amount: 1,
        }
    );
    assert_eq!(actions[1].action, Action::ApplyBulletTime);

    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::StrikeG, 1001),
        CombatCard::for_scenario(CardId::DefendG, 1002),
        CombatCard::for_scenario(CardId::Dash, 1003),
    ];
    crate::engine::action_handlers::execute_action(Action::ApplyBulletTime, &mut state);
    assert!(
        state
            .zones
            .hand
            .iter()
            .all(|card| card.cost_for_turn_java() == 0),
        "Java ApplyBulletTimeAction calls setCostForTurn(-9) on every current hand card"
    );
}

#[test]
fn tools_of_the_trade_matches_java_post_draw_cycle() {
    let tools = get_card_definition(CardId::ToolsOfTheTrade);
    assert_eq!(tools.name, "Tools of the Trade");
    assert_eq!(tools.card_type, CardType::Power);
    assert_eq!(tools.rarity, CardRarity::Rare);
    assert_eq!(tools.cost, 1);
    assert_eq!(tools.target, CardTarget::SelfTarget);
    assert_eq!(java_id(CardId::ToolsOfTheTrade), "Tools of the Trade");
    assert_eq!(
        build_java_id_map().get("Tools of the Trade"),
        Some(&CardId::ToolsOfTheTrade)
    );
    let mut tools_plus = CombatCard::for_scenario(CardId::ToolsOfTheTrade, 1004);
    tools_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&tools_plus), Some(0));

    let actions = resolve_card_play(
        CardId::ToolsOfTheTrade,
        &crate::test_support::blank_test_combat(),
        &tools_plus,
        None,
    );
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::ToolsOfTheTrade,
            amount: 1,
        }
    );

    let post_draw_actions = crate::content::powers::resolve_power_on_post_draw(
        PowerId::ToolsOfTheTrade,
        &crate::test_support::blank_test_combat(),
        0,
        2,
    );
    assert_eq!(post_draw_actions[0], Action::DrawCards(2));
    assert_eq!(
        post_draw_actions[1],
        Action::DiscardFromHand {
            amount: 2,
            random: false,
            end_turn: false,
        }
    );
}

#[test]
fn alchemize_matches_java_random_potion_action() {
    let alchemize = get_card_definition(CardId::Alchemize);
    assert_eq!(alchemize.name, "Alchemize");
    assert_eq!(alchemize.card_type, CardType::Skill);
    assert_eq!(alchemize.rarity, CardRarity::Rare);
    assert_eq!(alchemize.cost, 1);
    assert_eq!(alchemize.target, CardTarget::SelfTarget);
    assert!(alchemize.exhaust);
    assert!(alchemize.tags.contains(&CardTag::Healing));
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Alchemize,
        1005
    )));
    assert_eq!(java_id(CardId::Alchemize), "Venomology");
    assert_eq!(
        build_java_id_map().get("Venomology"),
        Some(&CardId::Alchemize)
    );
    let mut alchemize_plus = CombatCard::for_scenario(CardId::Alchemize, 1006);
    alchemize_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&alchemize_plus), Some(0));

    let actions = resolve_card_play(
        CardId::Alchemize,
        &crate::test_support::blank_test_combat(),
        &alchemize_plus,
        None,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, Action::ObtainPotion);
}

#[test]
fn alchemize_consumes_potion_rng_even_when_potion_is_not_obtained_like_java() {
    let mut sozu_state = crate::test_support::blank_test_combat();
    sozu_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::Sozu,
        ));
    let before_sozu_rng = sozu_state.rng.potion_rng.counter;
    crate::engine::action_handlers::cards::handle_obtain_potion(&mut sozu_state);
    assert!(
        sozu_state.rng.potion_rng.counter > before_sozu_rng,
        "Java Alchemize calls returnRandomPotion(true) before ObtainPotionAction checks Sozu"
    );
    assert!(sozu_state.entities.potions.iter().all(Option::is_none));

    let mut full_state = crate::test_support::blank_test_combat();
    full_state.entities.potions = vec![
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::FirePotion,
            10,
        )),
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::BlockPotion,
            11,
        )),
    ];
    let before_full_rng = full_state.rng.potion_rng.counter;
    let before_potions = full_state.entities.potions.clone();
    crate::engine::action_handlers::cards::handle_obtain_potion(&mut full_state);
    assert!(
        full_state.rng.potion_rng.counter > before_full_rng,
        "Java stores a concrete random potion in ObtainPotionAction even when obtainPotion later finds no empty slot"
    );
    assert_eq!(full_state.entities.potions, before_potions);
}

#[test]
fn distraction_matches_java_random_skill_free_for_turn() {
    let distraction = get_card_definition(CardId::Distraction);
    assert_eq!(distraction.name, "Distraction");
    assert_eq!(distraction.card_type, CardType::Skill);
    assert_eq!(distraction.rarity, CardRarity::Uncommon);
    assert_eq!(distraction.cost, 1);
    assert_eq!(distraction.target, CardTarget::None);
    assert!(distraction.exhaust);
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Distraction,
        1007
    )));
    assert_eq!(java_id(CardId::Distraction), "Distraction");
    assert_eq!(
        build_java_id_map().get("Distraction"),
        Some(&CardId::Distraction)
    );
    let mut distraction_plus = CombatCard::for_scenario(CardId::Distraction, 1008);
    distraction_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&distraction_plus), Some(0));

    let actions = resolve_card_play(
        CardId::Distraction,
        &crate::test_support::blank_test_combat(),
        &distraction_plus,
        None,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::MakeRandomCardInHand {
            card_type: Some(CardType::Skill),
            cost_for_turn: Some(0),
        }
    );

    let mut play_state = crate::test_support::blank_test_combat();
    play_state.meta.player_class = "Silent".to_string();
    play_state.zones.hand = vec![CombatCard::for_scenario(CardId::Distraction, 10080)];
    assert_eq!(play_state.rng.card_random_rng.counter, 0);
    crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut play_state)
        .expect("Distraction should be playable");
    assert_eq!(
        play_state.rng.card_random_rng.counter, 1,
        "Java Distraction.use calls returnTrulyRandomCardInCombat before queuing MakeTempCardInHandAction"
    );
    match play_state.pop_next_action() {
        Some(Action::MakeConstructedCopyInHand { original, amount }) => {
            assert_eq!(amount, 1);
            let generated_def = get_card_definition(original.id);
            assert_eq!(generated_def.card_type, CardType::Skill);
            let expected_cost_for_turn = i32::from(if generated_def.cost >= 0 {
                0
            } else {
                generated_def.cost
            });
            assert_eq!(
                original.cost_for_turn_java(),
                expected_cost_for_turn,
                "Java Distraction calls setCostForTurn(-99), which leaves X-cost/unplayable costs unchanged"
            );
        }
        other => panic!("Distraction should queue a concrete generated card, got {other:?}"),
    }
}

#[test]
fn corpse_explosion_matches_java_poison_then_death_power() {
    let corpse = get_card_definition(CardId::CorpseExplosion);
    assert_eq!(corpse.name, "Corpse Explosion");
    assert_eq!(corpse.card_type, CardType::Skill);
    assert_eq!(corpse.rarity, CardRarity::Rare);
    assert_eq!(corpse.cost, 2);
    assert_eq!(corpse.base_magic, 6);
    assert_eq!(corpse.target, CardTarget::Enemy);
    assert_eq!(corpse.upgrade_magic, 3);
    assert_eq!(java_id(CardId::CorpseExplosion), "Corpse Explosion");
    assert_eq!(
        build_java_id_map().get("Corpse Explosion"),
        Some(&CardId::CorpseExplosion)
    );
    assert!(crate::content::powers::is_debuff(
        PowerId::CorpseExplosion,
        1
    ));
    assert!(crate::content::powers::is_debuff_application(
        PowerId::CorpseExplosion,
        1
    ));

    let corpse_base = CombatCard::for_scenario(CardId::CorpseExplosion, 10090);
    let base_actions = resolve_card_play(
        CardId::CorpseExplosion,
        &crate::test_support::blank_test_combat(),
        &corpse_base,
        Some(1),
    );
    assert_eq!(base_actions.len(), 2);
    assert_eq!(
        base_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 1,
            power_id: PowerId::Poison,
            amount: 6,
        }
    );
    assert_eq!(
        base_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 1,
            power_id: PowerId::CorpseExplosion,
            amount: 1,
        }
    );

    let mut corpse_plus = CombatCard::for_scenario(CardId::CorpseExplosion, 1009);
    corpse_plus.upgrades = 1;
    let plus_actions = resolve_card_play(
        CardId::CorpseExplosion,
        &crate::test_support::blank_test_combat(),
        &corpse_plus,
        Some(1),
    );
    assert_eq!(plus_actions.len(), 2);
    assert_eq!(
        plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 1,
            power_id: PowerId::Poison,
            amount: 9,
        }
    );
    assert_eq!(
        plus_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 1,
            power_id: PowerId::CorpseExplosion,
            amount: 1,
        }
    );
}

#[test]
fn corpse_explosion_power_on_death_matches_java_max_hp_thorns_blast() {
    let mut state = crate::test_support::blank_test_combat();
    let mut exploding = crate::test_support::test_monster(EnemyId::JawWorm);
    exploding.id = 10;
    exploding.current_hp = 0;
    exploding.max_hp = 42;
    let mut survivor = crate::test_support::test_monster(EnemyId::JawWorm);
    survivor.id = 11;
    survivor.current_hp = 40;
    state.entities.monsters = vec![exploding, survivor];
    crate::content::powers::store::set_powers_for(
        &mut state,
        10,
        vec![Power {
            power_type: PowerId::CorpseExplosion,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    crate::engine::action_handlers::check_and_trigger_monster_death(&mut state, 10);

    assert!(state.entities.monsters[0].is_dying);
    assert_eq!(
        state.pop_next_action(),
        Some(Action::DamageAllEnemies {
            source: NO_SOURCE,
            damages: smallvec::smallvec![84, 84],
            damage_type: DamageType::Thorns,
            is_modified: false,
        })
    );

    let mut last_monster_state = crate::test_support::blank_test_combat();
    let mut last_monster = crate::test_support::test_monster(EnemyId::JawWorm);
    last_monster.id = 1;
    last_monster.current_hp = 0;
    last_monster_state.entities.monsters = vec![last_monster];
    crate::content::powers::store::set_powers_for(
        &mut last_monster_state,
        1,
        vec![Power {
            power_type: PowerId::CorpseExplosion,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    crate::engine::action_handlers::check_and_trigger_monster_death(&mut last_monster_state, 1);

    assert_eq!(
        last_monster_state.pop_next_action(),
        None,
        "Java CorpseExplosionPower skips the blast once all monsters are basically dead"
    );
}

#[test]
fn free_once_waives_energy_without_making_unplayable_cards_legal() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 0;
    for id in [CardId::DefendG, CardId::CorpseExplosion, CardId::WraithForm] {
        let mut card = CombatCard::for_scenario(id, 71);
        assert!(can_play_card(&card, &state).is_err());
        card.free_to_play_once = true;
        assert!(
            can_play_card(&card, &state).is_ok(),
            "{id:?} must be playable at zero energy"
        );
    }
    let mut unplayable = CombatCard::for_scenario(CardId::AscendersBane, 72);
    unplayable.free_to_play_once = true;
    assert!(can_play_card(&unplayable, &state).is_err());
}

#[test]
fn setup_matches_java_hand_select_to_draw_top_with_free_once() {
    let setup = get_card_definition(CardId::Setup);
    assert_eq!(setup.name, "Setup");
    assert_eq!(setup.card_type, CardType::Skill);
    assert_eq!(setup.rarity, CardRarity::Uncommon);
    assert_eq!(setup.cost, 1);
    assert_eq!(setup.target, CardTarget::None);
    assert_eq!(java_id(CardId::Setup), "Setup");
    assert_eq!(build_java_id_map().get("Setup"), Some(&CardId::Setup));

    let mut setup_plus = CombatCard::for_scenario(CardId::Setup, 1010);
    setup_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&setup_plus), Some(0));

    let actions = resolve_card_play(
        CardId::Setup,
        &crate::test_support::blank_test_combat(),
        &setup_plus,
        None,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::SuspendForHandSelect {
            min: 1,
            max: 1,
            can_cancel: false,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::Setup,
        }
    );

    let mut choice_state = crate::state::core::EngineState::PendingChoice(
        crate::state::core::PendingChoice::HandSelect {
            candidate_lineages: vec![crate::runtime::combat::CardLineageId::constructed_scenario(
                21,
            )],
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: crate::state::HandSelectReason::Setup,
        },
    );
    let mut combat_state = crate::test_support::blank_test_combat();
    let mut chosen = CombatCard::for_scenario(CardId::DefendG, 21);
    chosen.cost_for_turn = Some(0);
    combat_state.zones.hand = vec![chosen, CombatCard::for_scenario(CardId::Slice, 22)];

    crate::engine::pending_choices::handle_hand_select(
        &mut choice_state,
        &mut combat_state,
        &[
            crate::runtime::combat::CardLineageId::constructed_scenario(21),
            crate::runtime::combat::CardLineageId::constructed_scenario(22),
        ],
        1,
        true,
        false,
        crate::state::HandSelectReason::Setup,
        crate::state::core::ClientInput::SubmitSelection(
            crate::state::selection::SelectionResolution::card_lineages(
                crate::state::selection::SelectionScope::Hand,
                vec![crate::runtime::combat::CardLineageId::constructed_scenario(
                    21,
                )],
            ),
        ),
    )
    .expect("Setup selection should resolve");

    assert_eq!(
        choice_state,
        crate::state::core::EngineState::CombatProcessing
    );
    assert_eq!(combat_state.zones.hand.len(), 1);
    assert_eq!(
        combat_state.zones.draw_pile[0].lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(21)
    );
    assert!(
        combat_state.zones.draw_pile[0].free_to_play_once,
        "Java SetupAction checks AbstractCard.cost, so a temporary zero-cost turn override still becomes free once"
    );
}

#[test]
fn well_laid_plans_matches_java_retain_cards_power() {
    let plans = get_card_definition(CardId::WellLaidPlans);
    assert_eq!(plans.name, "Well Laid Plans");
    assert_eq!(plans.card_type, CardType::Power);
    assert_eq!(plans.rarity, CardRarity::Uncommon);
    assert_eq!(plans.cost, 1);
    assert_eq!(plans.base_magic, 1);
    assert_eq!(plans.target, CardTarget::None);
    assert_eq!(plans.upgrade_magic, 1);
    assert_eq!(java_id(CardId::WellLaidPlans), "Well Laid Plans");
    assert_eq!(
        build_java_id_map().get("Well Laid Plans"),
        Some(&CardId::WellLaidPlans)
    );

    let plans_base_card = CombatCard::for_scenario(CardId::WellLaidPlans, 10110);
    let base_actions = resolve_card_play(
        CardId::WellLaidPlans,
        &crate::test_support::blank_test_combat(),
        &plans_base_card,
        None,
    );
    assert_eq!(base_actions.len(), 1);
    assert_eq!(
        base_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::RetainCards,
            amount: 1,
        }
    );

    let mut plans_plus = CombatCard::for_scenario(CardId::WellLaidPlans, 1011);
    plans_plus.upgrades = 1;
    let plus_actions = resolve_card_play(
        CardId::WellLaidPlans,
        &crate::test_support::blank_test_combat(),
        &plans_plus,
        None,
    );
    assert_eq!(plus_actions.len(), 1);
    assert_eq!(
        plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::RetainCards,
            amount: 2,
        }
    );

    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![CombatCard::for_scenario(CardId::StrikeG, 31)];
    let retain_actions = crate::content::powers::silent::retain_cards::at_end_of_turn(&state, 0, 2);
    assert_eq!(retain_actions.len(), 1);
    assert_eq!(
        retain_actions[0],
        Action::SuspendForHandSelect {
            min: 0,
            max: 2,
            can_cancel: true,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::Retain,
        }
    );

    let mut pyramid_state = state.clone();
    pyramid_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::RunicPyramid,
        ));
    assert!(
        crate::content::powers::silent::retain_cards::at_end_of_turn(&pyramid_state, 0, 2)
            .is_empty(),
        "Java RetainCardPower does not open a retain choice under Runic Pyramid"
    );

    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Equilibrium,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    assert!(
        crate::content::powers::silent::retain_cards::at_end_of_turn(&state, 0, 2).is_empty(),
        "Java RetainCardPower does not open a retain choice while Equilibrium is active"
    );
}

#[test]
fn retain_selection_does_not_mark_ethereal_cards_like_java() {
    let mut choice_state = crate::state::core::EngineState::PendingChoice(
        crate::state::core::PendingChoice::HandSelect {
            candidate_lineages: vec![
                crate::runtime::combat::CardLineageId::constructed_scenario(41),
                crate::runtime::combat::CardLineageId::constructed_scenario(42),
            ],
            min_cards: 0,
            max_cards: 2,
            can_cancel: true,
            reason: crate::state::HandSelectReason::Retain,
        },
    );
    let mut combat_state = crate::test_support::blank_test_combat();
    combat_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Apparition, 41),
        CombatCard::for_scenario(CardId::StrikeG, 42),
    ];

    crate::engine::pending_choices::handle_hand_select(
        &mut choice_state,
        &mut combat_state,
        &[
            crate::runtime::combat::CardLineageId::constructed_scenario(41),
            crate::runtime::combat::CardLineageId::constructed_scenario(42),
        ],
        2,
        false,
        true,
        crate::state::HandSelectReason::Retain,
        crate::state::core::ClientInput::SubmitSelection(
            crate::state::selection::SelectionResolution::card_lineages(
                crate::state::selection::SelectionScope::Hand,
                vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(41),
                    crate::runtime::combat::CardLineageId::constructed_scenario(42),
                ],
            ),
        ),
    )
    .expect("retain selection should resolve");

    assert_eq!(
        combat_state.zones.hand[0].retain_override, None,
        "Java RetainCardsAction returns ethereal cards to hand but does not set retain"
    );
    assert_eq!(combat_state.zones.hand[1].retain_override, Some(true));
}

#[test]
fn nightmare_matches_java_card_and_power_payload_flow() {
    let nightmare = get_card_definition(CardId::Nightmare);
    assert_eq!(nightmare.name, "Nightmare");
    assert_eq!(nightmare.card_type, CardType::Skill);
    assert_eq!(nightmare.rarity, CardRarity::Rare);
    assert_eq!(nightmare.cost, 3);
    assert_eq!(nightmare.base_magic, 3);
    assert_eq!(nightmare.target, CardTarget::None);
    assert!(nightmare.exhaust);
    assert_eq!(java_id(CardId::Nightmare), "Night Terror");
    assert_eq!(
        build_java_id_map().get("Night Terror"),
        Some(&CardId::Nightmare)
    );

    let nightmare_base = CombatCard::for_scenario(CardId::Nightmare, 10120);
    let base_actions = resolve_card_play(
        CardId::Nightmare,
        &crate::test_support::blank_test_combat(),
        &nightmare_base,
        None,
    );
    assert_eq!(base_actions.len(), 1);
    assert_eq!(base_actions[0].action, Action::Nightmare { amount: 3 });

    let mut nightmare_plus = CombatCard::for_scenario(CardId::Nightmare, 1012);
    nightmare_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&nightmare_plus), Some(2));
    let plus_actions = resolve_card_play(
        CardId::Nightmare,
        &crate::test_support::blank_test_combat(),
        &nightmare_plus,
        None,
    );
    assert_eq!(plus_actions.len(), 1);
    assert_eq!(plus_actions[0].action, Action::Nightmare { amount: 3 });

    let mut state = crate::test_support::blank_test_combat();
    let mut copied = CombatCard::for_scenario(CardId::Bash, 51);
    copied.upgrades = 1;
    copied.misc_value = 7;
    copied.base_damage_override = Some(42);
    copied.cost_modifier = -1;
    copied.cost_for_turn = Some(0);
    copied.base_damage_mut = 99;
    copied.free_to_play_once = true;
    state.zones.hand = vec![copied.clone()];

    crate::engine::action_handlers::cards::handle_nightmare(3, &mut state);
    match state.pop_next_action() {
        Some(Action::ApplyPowerWithPayload {
            source,
            target,
            power_id,
            amount,
            instance_id,
            payload:
                crate::runtime::combat::PowerPayload::Card(crate::runtime::combat::CombatCard {
                    id,
                    upgrades,
                    misc_value,
                    base_damage_override,
                    cost_modifier,
                    cost_for_turn,
                    base_damage_mut,
                    free_to_play_once,
                    ..
                }),
            ..
        }) => {
            assert_eq!(source, 0);
            assert_eq!(target, 0);
            assert_eq!(power_id, PowerId::Nightmare);
            assert_eq!(amount, 3);
            assert_eq!(instance_id, Some(1));
            assert_eq!(id, CardId::Bash);
            assert_eq!(upgrades, 1);
            assert_eq!(misc_value, 7);
            assert_eq!(base_damage_override, Some(42));
            assert_eq!(cost_modifier, -1);
            assert_eq!(cost_for_turn, None);
            assert_eq!(base_damage_mut, 0);
            assert!(free_to_play_once);
        }
        other => panic!("expected Nightmare ApplyPowerWithPayload, got {other:?}"),
    }
}

#[test]
fn nightmare_selection_returns_original_and_start_turn_copies_payload() {
    let mut choice_state = crate::state::core::EngineState::PendingChoice(
        crate::state::core::PendingChoice::HandSelect {
            candidate_lineages: vec![
                crate::runtime::combat::CardLineageId::constructed_scenario(61),
                crate::runtime::combat::CardLineageId::constructed_scenario(62),
            ],
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: crate::state::HandSelectReason::Nightmare { amount: 2 },
        },
    );
    let mut combat_state = crate::test_support::blank_test_combat();
    combat_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::StrikeG, 61),
        CombatCard::for_scenario(CardId::DefendG, 62),
    ];

    crate::engine::pending_choices::handle_hand_select(
        &mut choice_state,
        &mut combat_state,
        &[
            crate::runtime::combat::CardLineageId::constructed_scenario(61),
            crate::runtime::combat::CardLineageId::constructed_scenario(62),
        ],
        1,
        true,
        false,
        crate::state::HandSelectReason::Nightmare { amount: 2 },
        crate::state::core::ClientInput::SubmitSelection(
            crate::state::selection::SelectionResolution::card_lineages(
                crate::state::selection::SelectionScope::Hand,
                vec![crate::runtime::combat::CardLineageId::constructed_scenario(
                    61,
                )],
            ),
        ),
    )
    .expect("Nightmare hand selection should resolve");

    assert_eq!(
        choice_state,
        crate::state::core::EngineState::CombatProcessing
    );
    assert_eq!(
        combat_state
            .zones
            .hand
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        vec![
            crate::runtime::combat::CardLineageId::constructed_scenario(62),
            crate::runtime::combat::CardLineageId::constructed_scenario(61),
        ],
        "Java NightmareAction returns the selected original with hand.addToHand after hand-select removal"
    );

    let apply = combat_state
        .pop_next_action()
        .expect("Nightmare selection should queue power application");
    crate::engine::action_handlers::execute_action(apply, &mut combat_state);
    let power = crate::content::powers::store::powers_for(&combat_state, 0)
        .and_then(|powers| powers.iter().find(|p| p.power_type == PowerId::Nightmare))
        .cloned()
        .expect("Nightmare power should be applied");

    let start_actions =
        crate::content::powers::resolve_power_instance_at_turn_start(&power, &mut combat_state, 0);
    assert_eq!(start_actions.len(), 2);
    match &start_actions[0] {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::StrikeG);
            assert_eq!(*amount, 2);
        }
        other => panic!("expected Nightmare to make copies at start of turn, got {other:?}"),
    }
    assert_eq!(
        start_actions[1],
        Action::RemovePowerInstance {
            target: 0,
            power_id: PowerId::Nightmare,
            instance_id: power.instance_id.unwrap(),
        }
    );
}

#[test]
fn piercing_wail_matches_java_artifact_and_shackled_rules() {
    let piercing = get_card_definition(CardId::PiercingWail);
    assert_eq!(piercing.name, "Piercing Wail");
    assert_eq!(piercing.card_type, CardType::Skill);
    assert_eq!(piercing.rarity, CardRarity::Common);
    assert_eq!(piercing.cost, 1);
    assert_eq!(piercing.base_magic, 6);
    assert_eq!(piercing.upgrade_magic, 2);
    assert!(piercing.exhaust);
    assert_eq!(java_id(CardId::PiercingWail), "PiercingWail");
    assert!(
        !crate::content::powers::is_debuff_application(PowerId::Shackled, 6),
        "Java ApplyPowerAction does not let Artifact block GainStrengthPower/Shackled"
    );

    let mut state = crate::test_support::blank_test_combat();
    let mut no_artifact = crate::test_support::test_monster(EnemyId::JawWorm);
    no_artifact.id = 7;
    let mut with_artifact = crate::test_support::test_monster(EnemyId::JawWorm);
    with_artifact.id = 8;
    state.entities.monsters = vec![no_artifact, with_artifact];
    state.entities.power_db.insert(
        8,
        vec![Power {
            power_type: PowerId::Artifact,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    let actions = resolve_card_play(
        CardId::PiercingWail,
        &state,
        &CombatCard::for_scenario(CardId::PiercingWail, 908),
        None,
    );
    assert_eq!(actions.len(), 3);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Strength,
            amount: -6,
        }
    );
    assert_eq!(
        actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 8,
            power_id: PowerId::Strength,
            amount: -6,
        }
    );
    assert_eq!(
        actions[2].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Shackled,
            amount: 6,
        },
        "Java queues GainStrengthPower only for monsters that lack Artifact at use time"
    );

    let mut upgraded = CombatCard::for_scenario(CardId::PiercingWail, 909);
    upgraded.upgrades = 1;
    let upgraded_actions = resolve_card_play(CardId::PiercingWail, &state, &upgraded, None);
    assert_eq!(
        upgraded_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Strength,
            amount: -8,
        }
    );

    let mut artifact_state = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 7;
    artifact_state.entities.monsters = vec![monster];
    artifact_state.entities.power_db.insert(
        7,
        vec![Power {
            power_type: PowerId::Artifact,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Shackled,
            amount: 6,
        },
        &mut artifact_state,
    );
    assert_eq!(
        crate::content::powers::store::power_amount(&artifact_state, 7, PowerId::Artifact),
        1
    );
    assert_eq!(
        crate::content::powers::store::power_amount(&artifact_state, 7, PowerId::Shackled),
        6
    );
}

#[test]
fn reflex_and_tactician_manual_discard_hooks_match_java_order() {
    let reflex = get_card_definition(CardId::Reflex);
    assert_eq!(reflex.cost, -2);
    assert_eq!(reflex.card_type, CardType::Skill);
    assert_eq!(reflex.rarity, CardRarity::Uncommon);
    assert_eq!(reflex.base_magic, 2);
    assert_eq!(reflex.upgrade_magic, 1);

    let tactician = get_card_definition(CardId::Tactician);
    assert_eq!(tactician.cost, -2);
    assert_eq!(tactician.base_magic, 1);
    assert_eq!(tactician.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Reflex), "Reflex");
    assert_eq!(java_id(CardId::Tactician), "Tactician");
    assert_eq!(build_java_id_map().get("Reflex"), Some(&CardId::Reflex));
    assert_eq!(
        build_java_id_map().get("Tactician"),
        Some(&CardId::Tactician)
    );

    let mut reflex_state = crate::test_support::blank_test_combat();
    reflex_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::Tingsha,
        ));
    reflex_state.zones.hand = vec![CombatCard::for_scenario(CardId::Reflex, 870)];
    crate::engine::action_handlers::cards::handle_discard_card(
        crate::runtime::combat::CardLineageId::constructed_scenario(870),
        &mut reflex_state,
    );

    assert_eq!(reflex_state.turn.counters.cards_discarded_this_turn, 1);
    assert_eq!(reflex_state.zones.discard_pile[0].id, CardId::Reflex);
    assert!(matches!(
        reflex_state.pop_next_action(),
        Some(Action::DamageRandomEnemy { .. }),
    ));
    assert_eq!(
        reflex_state.pop_next_action(),
        Some(Action::DrawCards(2)),
        "Java DiscardSpecificCardAction/GamblingChipAction increments discard before Reflex.triggerOnManualDiscard"
    );

    let mut reflex_plus_state = crate::test_support::blank_test_combat();
    let mut reflex_plus = CombatCard::for_scenario(CardId::Reflex, 872);
    reflex_plus.upgrades = 1;
    reflex_plus_state.zones.hand = vec![reflex_plus];
    crate::engine::action_handlers::cards::handle_discard_card(
        crate::runtime::combat::CardLineageId::constructed_scenario(872),
        &mut reflex_plus_state,
    );
    assert_eq!(
        reflex_plus_state.pop_next_action(),
        Some(Action::DrawCards(3)),
        "Reflex+ manual discard derives magic from definition + upgrades, not prefilled render fields"
    );

    let mut tactician_base_state = crate::test_support::blank_test_combat();
    tactician_base_state.zones.hand = vec![CombatCard::for_scenario(CardId::Tactician, 873)];
    crate::engine::action_handlers::cards::handle_discard_card(
        crate::runtime::combat::CardLineageId::constructed_scenario(873),
        &mut tactician_base_state,
    );
    assert_eq!(
        tactician_base_state.pop_next_action(),
        Some(Action::GainEnergy { amount: 1 }),
        "Tactician base manual discard derives magic from definition + upgrades"
    );

    let mut tactician_state = crate::test_support::blank_test_combat();
    tactician_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::ToughBandages,
        ));
    let mut tactician_plus = CombatCard::for_scenario(CardId::Tactician, 871);
    tactician_plus.upgrades = 1;
    tactician_state.zones.hand = vec![tactician_plus];
    crate::engine::action_handlers::cards::handle_discard_card(
        crate::runtime::combat::CardLineageId::constructed_scenario(871),
        &mut tactician_state,
    );

    assert_eq!(tactician_state.turn.counters.cards_discarded_this_turn, 1);
    assert_eq!(
        tactician_state.pop_next_action(),
        Some(Action::GainEnergy { amount: 2 }),
        "Java Tactician.triggerOnManualDiscard uses addToTop, so it executes before relic addToBot actions queued by incrementDiscard"
    );
    assert_eq!(
        tactician_state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 3,
        })
    );
}

#[test]
fn discard_action_manual_discard_hooks_run_card_before_relic_hooks() {
    let mut reflex_state = crate::test_support::blank_test_combat();
    reflex_state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    reflex_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::Tingsha,
        ));
    reflex_state.zones.hand = vec![CombatCard::for_scenario(CardId::Reflex, 872)];

    crate::engine::action_handlers::cards::handle_discard_from_hand(
        1,
        false,
        false,
        &mut reflex_state,
    );

    assert_eq!(reflex_state.turn.counters.cards_discarded_this_turn, 1);
    assert_eq!(
        reflex_state.pop_next_action(),
        Some(Action::DrawCards(2)),
        "Java DiscardAction calls triggerOnManualDiscard before incrementDiscard"
    );
    assert!(matches!(
        reflex_state.pop_next_action(),
        Some(Action::DamageRandomEnemy { .. }),
    ));

    let mut chip_state = crate::test_support::blank_test_combat();
    chip_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::ToughBandages,
        ));
    let mut tactician_plus = CombatCard::for_scenario(CardId::Tactician, 873);
    tactician_plus.upgrades = 1;
    chip_state.zones.hand = vec![tactician_plus];
    let mut engine_state = crate::state::EngineState::CombatProcessing;

    crate::engine::pending_choices::handle_hand_select(
        &mut engine_state,
        &mut chip_state,
        &[crate::runtime::combat::CardLineageId::constructed_scenario(
            873,
        )],
        1,
        false,
        true,
        crate::state::HandSelectReason::GamblingChip,
        crate::state::ClientInput::SubmitSelection(
            crate::state::selection::SelectionResolution::card_lineages(
                crate::state::selection::SelectionScope::Hand,
                vec![crate::runtime::combat::CardLineageId::constructed_scenario(
                    873,
                )],
            ),
        ),
    )
    .expect("valid Gambling Chip hand selection");

    assert_eq!(chip_state.turn.counters.cards_discarded_this_turn, 1);
    assert_eq!(
        chip_state.pop_next_action(),
        Some(Action::GainEnergy { amount: 2 }),
        "Java GamblingChipAction queues DrawCardAction to top before discards, so later Tactician addToTop executes first"
    );
    assert_eq!(chip_state.pop_next_action(), Some(Action::DrawCards(1)));
    assert_eq!(
        chip_state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 3,
        })
    );
}

#[test]
fn discard_action_random_end_turn_path_keeps_card_hook_without_relic_hook() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::Tingsha,
        ));
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Reflex, 874),
        CombatCard::for_scenario(CardId::Reflex, 875),
    ];

    crate::engine::action_handlers::cards::handle_discard_from_hand(1, true, true, &mut state);

    assert_eq!(state.turn.counters.cards_discarded_this_turn, 1);
    assert_eq!(state.zones.discard_pile.len(), 1);
    assert_eq!(
        state.pop_next_action(),
        Some(Action::DrawCards(2)),
        "Java DiscardAction random branch calls triggerOnManualDiscard even when endTurn=true"
    );
    assert_eq!(
        state.pop_next_action(),
        None,
        "GameActionManager.incrementDiscard(endTurn=true) suppresses relic onManualDiscard"
    );
}

#[test]
fn exhaust_from_hand_matches_java_auto_and_any_number_paths() {
    let mut auto_state = crate::test_support::blank_test_combat();
    auto_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 860),
        CombatCard::for_scenario(CardId::Defend, 861),
    ];

    crate::engine::action_handlers::cards::handle_exhaust_from_hand(
        2,
        false,
        false,
        false,
        &mut auto_state,
    );

    assert!(auto_state.zones.hand.is_empty());
    assert_eq!(
        auto_state
            .zones
            .exhaust_pile
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        vec![
            crate::runtime::combat::CardLineageId::constructed_scenario(861),
            crate::runtime::combat::CardLineageId::constructed_scenario(860),
        ],
        "Java ExhaustAction auto path repeatedly moves hand.getTopCard when hand.size() <= amount"
    );
    assert_eq!(auto_state.pop_next_action(), None);

    let mut any_number_state = crate::test_support::blank_test_combat();
    any_number_state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 862)];
    crate::engine::action_handlers::cards::handle_exhaust_from_hand(
        3,
        false,
        true,
        true,
        &mut any_number_state,
    );

    assert!(matches!(
        any_number_state.pop_next_action(),
        Some(Action::SuspendForHandSelect {
            min: 0,
            max: 3,
            can_cancel: true,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::Exhaust,
        })
    ));
}

#[test]
fn upgraded_blind_and_trip_enqueue_apply_power_for_every_monster_like_java() {
    let mut state = crate::test_support::blank_test_combat();
    let mut zero_hp_not_dying = crate::test_support::test_monster(EnemyId::JawWorm);
    zero_hp_not_dying.id = 810;
    zero_hp_not_dying.current_hp = 0;
    zero_hp_not_dying.is_dying = false;
    let mut half_dead = crate::test_support::test_monster(EnemyId::Darkling);
    half_dead.id = 811;
    half_dead.half_dead = true;
    state.entities.monsters = vec![zero_hp_not_dying, half_dead];

    let mut blind_plus = CombatCard::for_scenario(CardId::Blind, 812);
    blind_plus.upgrades = 1;
    let blind_actions = resolve_card_play(CardId::Blind, &state, &blind_plus, None);
    assert_eq!(
        blind_actions.len(),
        2,
        "Java Blind+ loops over monsters.monsters and lets ApplyPowerAction handle dead/escaped filtering"
    );
    assert!(matches!(
        blind_actions[0].action,
        Action::ApplyPower {
            target: 810,
            power_id: PowerId::Weak,
            ..
        }
    ));
    assert!(matches!(
        blind_actions[1].action,
        Action::ApplyPower {
            target: 811,
            power_id: PowerId::Weak,
            ..
        }
    ));

    let mut trip_plus = CombatCard::for_scenario(CardId::Trip, 813);
    trip_plus.upgrades = 1;
    let trip_actions = resolve_card_play(CardId::Trip, &state, &trip_plus, None);
    assert_eq!(
        trip_actions.len(),
        2,
        "Java Trip+ loops over monsters.monsters and lets ApplyPowerAction handle dead/escaped filtering"
    );
    assert!(matches!(
        trip_actions[0].action,
        Action::ApplyPower {
            target: 810,
            power_id: PowerId::Vulnerable,
            ..
        }
    ));
    assert!(matches!(
        trip_actions[1].action,
        Action::ApplyPower {
            target: 811,
            power_id: PowerId::Vulnerable,
            ..
        }
    ));
}

#[test]
fn put_on_deck_action_matches_java_rng_and_selection_edges() {
    let mut one_card_state = crate::test_support::blank_test_combat();
    one_card_state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 400)];
    one_card_state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Defend, 401)]).into();
    assert_eq!(one_card_state.rng.card_random_rng.counter, 0);

    crate::engine::action_handlers::cards::handle_put_on_deck(1, false, &mut one_card_state);

    assert!(one_card_state.zones.hand.is_empty());
    assert_eq!(
        one_card_state.zones.draw_pile[0].lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(400)
    );
    assert_eq!(
        one_card_state.zones.draw_pile[1].lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(401)
    );
    assert_eq!(
        one_card_state.rng.card_random_rng.counter, 1,
        "Java PutOnDeckAction uses getRandomCard(cardRandomRng) when hand size <= amount"
    );

    let mut two_card_state = crate::test_support::blank_test_combat();
    two_card_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 402),
        CombatCard::for_scenario(CardId::Defend, 403),
    ];
    crate::engine::action_handlers::cards::handle_put_on_deck(1, false, &mut two_card_state);
    assert_eq!(two_card_state.zones.hand.len(), 2);
    assert_eq!(two_card_state.rng.card_random_rng.counter, 0);
    assert!(matches!(
        two_card_state.pop_next_action(),
        Some(Action::SuspendForHandSelect {
            min: 1,
            max: 1,
            can_cancel: false,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::PutOnDrawPile,
        })
    ));

    let mut fallback_state = crate::test_support::blank_test_combat();
    fallback_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 404),
        CombatCard::for_scenario(CardId::Defend, 405),
    ];
    crate::engine::action_handlers::cards::handle_put_on_deck(2, false, &mut fallback_state);
    assert_eq!(
        fallback_state.zones.hand.len(),
        1,
        "Java PutOnDeckAction fallback loop checks the shrinking hand size each iteration"
    );
    assert_eq!(fallback_state.zones.draw_pile.len(), 1);
    assert_eq!(fallback_state.rng.card_random_rng.counter, 1);
}

#[test]
fn apotheosis_uses_combat_wide_upgrade_action_not_armaments_hand_action() {
    let state = crate::test_support::blank_test_combat();
    let base = CombatCard::for_scenario(CardId::Apotheosis, 900);
    let actions = resolve_card_play(CardId::Apotheosis, &state, &base, None);

    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::UpgradeAllCardsInCombat,
        "Java ApotheosisAction upgrades hand, draw, discard, and exhaust; Blessing of the Forge is the ArmamentsAction(true) hand-only path"
    );

    let mut upgraded = base.clone();
    upgraded.upgrades = 1;
    assert_eq!(base.get_cost(), 2);
    assert_eq!(upgraded.get_cost(), 1, "Java Apotheosis.upgradeBaseCost(1)");
}
