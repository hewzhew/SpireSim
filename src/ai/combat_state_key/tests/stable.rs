use super::*;

#[test]
fn stable_outcome_key_ignores_player_resources_potions_and_runtime_noise() {
    let mut baseline = blank_test_combat();
    baseline
        .entities
        .monsters
        .push(planned_monster(EnemyId::Cultist, 3));
    baseline
        .zones
        .hand
        .push(CombatCard::for_scenario(CardId::Strike, 1));

    let mut variant = baseline.clone();
    variant.entities.player.current_hp -= 7;
    variant.entities.player.block = 12;
    variant.entities.potions = vec![Some(Potion::new(PotionId::SteroidPotion, 1)), None, None];
    variant.zones.queued_cards.push_back(QueuedCardPlay {
        card: CombatCard::for_scenario(CardId::Strike, 77),
        target: None,
        energy_on_use: 1,
        ignore_energy_total: false,
        autoplay: false,
        random_target: false,
        is_end_turn_autoplay: false,
        purge_on_use: false,
        source: QueuedCardSource::Normal,
    });
    variant.allocate_combat_generated_lineage();
    variant.queue_action_back(crate::runtime::action::Action::GainEnergy { amount: 1 });

    assert_eq!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &baseline),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &variant),
    );
    assert_ne!(
        combat_dominance_key(&EngineState::CombatPlayerTurn, &baseline),
        combat_dominance_key(&EngineState::CombatPlayerTurn, &variant),
    );
}

#[test]
fn stable_outcome_key_keeps_future_relevant_monster_card_and_turn_state() {
    let mut baseline = blank_test_combat();
    baseline
        .entities
        .monsters
        .push(planned_monster(EnemyId::Cultist, 3));
    baseline
        .zones
        .hand
        .push(CombatCard::for_scenario(CardId::Rampage, 1));

    let mut monster_variant = baseline.clone();
    monster_variant.entities.monsters[0].current_hp -= 5;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &baseline),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &monster_variant),
    );

    let mut card_variant = baseline.clone();
    card_variant.zones.hand[0].base_damage_mut = 13;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &baseline),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &card_variant),
    );

    let mut turn_variant = baseline.clone();
    turn_variant.turn.energy = 1;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &baseline),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &turn_variant),
    );
}

#[test]
fn stable_outcome_key_ignores_card_instance_ids_but_keeps_rng() {
    let mut baseline = blank_test_combat();
    baseline
        .entities
        .monsters
        .push(planned_monster(EnemyId::Cultist, 3));
    baseline
        .zones
        .hand
        .push(CombatCard::for_scenario(CardId::Rampage, 11));

    let mut uuid_variant = baseline.clone();
    uuid_variant.zones.hand[0].lineage =
        crate::runtime::combat::CardLineageId::constructed_scenario(999);
    uuid_variant.allocate_combat_generated_lineage();
    assert_eq!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &baseline),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &uuid_variant),
    );

    let mut rng_variant = baseline.clone();
    rng_variant.rng.ai_rng.counter += 1;
    assert_ne!(
        stable_outcome_key(&EngineState::CombatPlayerTurn, &baseline),
        stable_outcome_key(&EngineState::CombatPlayerTurn, &rng_variant),
    );
}

#[test]
fn stable_outcome_key_normalizes_hand_and_discovery_choice_order() {
    let mut baseline = blank_test_combat();
    baseline
        .entities
        .monsters
        .push(planned_monster(EnemyId::Cultist, 3));
    baseline.zones.hand.extend([
        CombatCard::for_scenario(CardId::Strike, 11),
        CombatCard::for_scenario(CardId::Defend, 12),
    ]);
    let hand_lineages = baseline
        .zones
        .hand
        .iter()
        .map(|card| card.lineage)
        .collect::<Vec<_>>();

    assert_eq!(
        stable_outcome_key(
            &EngineState::PendingChoice(PendingChoice::HandSelect {
                candidate_lineages: hand_lineages.clone(),
                min_cards: 0,
                max_cards: 1,
                can_cancel: true,
                reason: crate::state::core::HandSelectReason::Discard,
            }),
            &baseline,
        ),
        stable_outcome_key(
            &EngineState::PendingChoice(PendingChoice::HandSelect {
                candidate_lineages: hand_lineages.into_iter().rev().collect(),
                min_cards: 0,
                max_cards: 1,
                can_cancel: true,
                reason: crate::state::core::HandSelectReason::Discard,
            }),
            &baseline,
        ),
    );

    assert_eq!(
        stable_outcome_key(
            &EngineState::PendingChoice(PendingChoice::DiscoverySelect(
                crate::state::core::DiscoveryChoiceState {
                    cards: vec![CardId::Strike, CardId::Defend, CardId::Bash],
                    colorless: false,
                    card_type: None,
                    amount: 1,
                    can_skip: false,
                },
            )),
            &baseline,
        ),
        stable_outcome_key(
            &EngineState::PendingChoice(PendingChoice::DiscoverySelect(
                crate::state::core::DiscoveryChoiceState {
                    cards: vec![CardId::Bash, CardId::Strike, CardId::Defend],
                    colorless: false,
                    card_type: None,
                    amount: 1,
                    can_skip: false,
                },
            )),
            &baseline,
        ),
    );
}

#[test]
fn stable_outcome_key_keeps_scry_order_and_postcombat_gold_meta() {
    let mut baseline = blank_test_combat();
    baseline
        .entities
        .monsters
        .push(planned_monster(EnemyId::Cultist, 3));

    assert_ne!(
        stable_outcome_key(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards: vec![CardId::Strike, CardId::Defend],
                card_lineages: [1, 2].map(crate::test_support::scenario_lineage).to_vec(),
            }),
            &baseline,
        ),
        stable_outcome_key(
            &EngineState::PendingChoice(PendingChoice::ScrySelect {
                cards: vec![CardId::Defend, CardId::Strike],
                card_lineages: [2, 1].map(crate::test_support::scenario_lineage).to_vec(),
            }),
            &baseline,
        ),
    );

    let mut postcombat_variant = baseline.clone();
    postcombat_variant
        .zones
        .hand
        .push(CombatCard::for_scenario(CardId::Strike, 99));
    postcombat_variant.entities.monsters[0].current_hp -= 5;
    assert_eq!(
        stable_outcome_key(&EngineState::MapNavigation, &baseline),
        stable_outcome_key(&EngineState::MapNavigation, &postcombat_variant),
    );

    let mut gold_variant = baseline.clone();
    gold_variant.entities.player.gold += 25;
    assert_ne!(
        stable_outcome_key(&EngineState::MapNavigation, &baseline),
        stable_outcome_key(&EngineState::MapNavigation, &gold_variant),
    );

    let mut meta_variant = baseline.clone();
    meta_variant
        .meta
        .meta_changes
        .push(crate::runtime::combat::MetaChange::AddCardToMasterDeck(
            crate::test_support::test_master_deck_acquisition(CardId::Strike, 999),
        ));
    assert_ne!(
        stable_outcome_key(&EngineState::MapNavigation, &baseline),
        stable_outcome_key(&EngineState::MapNavigation, &meta_variant),
    );
}

#[test]
fn stable_outcome_key_treats_combat_processing_as_unstable_not_game_over() {
    let mut baseline = blank_test_combat();
    baseline
        .entities
        .monsters
        .push(planned_monster(EnemyId::Cultist, 3));

    let unstable = diagnostic_outcome_key(&EngineState::CombatProcessing, &baseline);
    let game_over = stable_outcome_key(
        &EngineState::GameOver(crate::state::core::RunResult::Defeat),
        &baseline,
    );

    assert!(unstable.diagnostic_string().contains("scope=Unstable"));
    assert_ne!(unstable, game_over);
    assert_eq!(
        stable_dominance_bucket_key(&EngineState::CombatProcessing, &baseline),
        None,
    );
}

#[test]
fn permanent_allocation_and_acquisition_source_are_part_of_state_identity() {
    let base = blank_test_combat();
    let mut changed = base.clone();
    changed.meta.master_deck_lineage_allocator.allocate();
    let engine = EngineState::CombatPlayerTurn;
    assert_ne!(
        combat_exact_state_key(&engine, &base),
        combat_exact_state_key(&engine, &changed)
    );
    assert_ne!(
        stable_outcome_key(&EngineState::MapNavigation, &base),
        stable_outcome_key(&EngineState::MapNavigation, &changed)
    );
    assert_ne!(
        stable_outcome_key(&engine, &base),
        stable_outcome_key(&engine, &changed)
    );
    let receipt = crate::test_support::test_master_deck_acquisition(CardId::Parasite, 99);
    changed
        .meta
        .meta_changes
        .push(crate::runtime::combat::MetaChange::AddCardToMasterDeck(
            receipt,
        ));
    let mut other = changed.clone();
    if let crate::runtime::combat::MetaChange::AddCardToMasterDeck(receipt) =
        &mut other.meta.meta_changes[0]
    {
        receipt.monster_slot = 1;
    }
    assert_ne!(
        combat_exact_state_key(&engine, &other),
        combat_exact_state_key(&engine, &changed)
    );
    assert_ne!(
        stable_outcome_key(&EngineState::MapNavigation, &other),
        stable_outcome_key(&EngineState::MapNavigation, &changed)
    );
}
