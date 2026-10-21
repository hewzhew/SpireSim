use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::potions::{Potion, PotionId};
use crate::content::powers::store;
use crate::content::relics::{RelicId, RelicState};
use crate::runtime::combat::{CombatRng, Power};
use crate::runtime::rng::RngPool;

#[test]
fn public_state_exposes_committed_smoke_escape_before_settlement() {
    use crate::engine::core::tick_until_stable_turn;
    use crate::state::core::{ClientInput, EngineState};

    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.monsters = vec![crate::test_support::planned_monster(EnemyId::JawWorm, 1)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
    combat.entities.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 1))];
    let mut engine = EngineState::CombatPlayerTurn;
    let initial = public_combat_state_v1(&combat);
    assert!(!initial.turn.player_escaping);
    let initial_json = serde_json::to_value(&initial).unwrap();
    assert!(initial_json["turn"].get("player_escaping").is_none());
    assert_eq!(
        serde_json::from_value::<PublicCombatStateV1>(initial_json).unwrap(),
        initial,
    );

    assert!(tick_until_stable_turn(
        &mut engine,
        &mut combat,
        ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
    ));
    let before_read = combat.clone();
    let observed = public_combat_state_v1(&combat);
    assert_eq!(combat, before_read, "observing escape must not settle it");
    assert!(observed.turn.player_escaping);
    assert_eq!(
        serde_json::to_value(&observed).unwrap()["turn"]["player_escaping"],
        true,
    );
    assert_eq!(observed.turn.phase, CombatPhase::PlayerTurn);
    assert_eq!(observed.cards.hand.cards.len(), 1);
    assert_eq!(
        crate::sim::combat_legal_actions::engine_atomic_actions(&engine, &combat),
        vec![ClientInput::EndTurn],
    );
}

#[test]
fn constricted_source_uses_public_monster_identity_without_runtime_handles() {
    use crate::engine::action_handlers::powers::handle_apply_power;
    let mut state = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::SpireGrowth);
    monster.id = 72;
    state.entities.monsters = vec![monster];
    handle_apply_power(72, 0, PowerId::Constricted, 10, &mut state);
    let observed = learning_powers(&state, 0);
    assert_eq!(
        observed[0].source_creature,
        Some(CombatLearningCreatureV1::Monster { monster_index: 0 })
    );
    state.entities.monsters[0].id = 49;
    state.entities.power_db.get_mut(&0).unwrap()[0].payload = PowerPayload::SourceEntity(49);
    assert_eq!(observed, learning_powers(&state, 0));
    state.entities.monsters.clear();
    assert_eq!(
        learning_powers(&state, 0)[0].source_creature,
        Some(CombatLearningCreatureV1::Unobserved)
    );
}

fn test_power(power_type: PowerId, instance_id: u32, amount: i32) -> Power {
    Power {
        power_type,
        instance_id: Some(instance_id),
        amount,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    }
}

#[test]
fn private_rng_and_execution_handles_do_not_change_public_state() {
    let mut left = crate::test_support::blank_test_combat();
    left.zones.hand = vec![CombatCard::for_scenario(CardId::Bash, 1)];
    left.entities.potions = vec![Some(Potion::new(PotionId::FruitJuice, 41))];
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 7;
    left.entities.monsters.push(monster);
    let mut right = left.clone();
    right.zones.hand[0].lineage = crate::test_support::scenario_lineage(99);
    right.entities.potions[0].as_mut().unwrap().uuid = 199;
    right.entities.monsters[0].id = 299;
    right.rng = CombatRng::new(RngPool::new(123_456));

    assert_eq!(
        public_combat_state_v1(&left),
        public_combat_state_v1(&right)
    );
}

#[test]
fn unordered_public_pile_order_does_not_change_public_state() {
    let mut left = crate::test_support::blank_test_combat();
    left.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Bash, 1),
        CombatCard::for_scenario(CardId::Defend, 2),
    ]
    .into();
    left.zones.exhaust_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 3),
        CombatCard::for_scenario(CardId::AscendersBane, 4),
    ]
    .into();
    let mut right = left.clone();
    let mut discard = std::mem::take(&mut right.zones.discard_pile).into_vec();
    discard.reverse();
    right.zones.discard_pile = discard.into();
    right.zones.exhaust_pile.reverse();

    assert_eq!(
        public_combat_state_v1(&left),
        public_combat_state_v1(&right)
    );
}

#[test]
fn public_master_deck_ordinal_hides_private_source_order() {
    let mut left = crate::test_support::blank_test_combat();
    left.meta.master_deck_snapshot = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Bash, 20),
        CombatCard::for_scenario(CardId::Strike, 30),
    ]
    .into();
    let mut right = left.clone();
    right.meta.master_deck_snapshot = vec![
        CombatCard::for_scenario(CardId::Strike, 30),
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Bash, 20),
    ]
    .into();

    let left_bash =
        public_master_deck_ordinal_for_lineage_v1(&left, crate::test_support::scenario_lineage(20));
    let right_bash = public_master_deck_ordinal_for_lineage_v1(
        &right,
        crate::test_support::scenario_lineage(20),
    );
    let left_first_strike =
        public_master_deck_ordinal_for_lineage_v1(&left, crate::test_support::scenario_lineage(10));
    let right_first_strike = public_master_deck_ordinal_for_lineage_v1(
        &right,
        crate::test_support::scenario_lineage(10),
    );
    let left_second_strike =
        public_master_deck_ordinal_for_lineage_v1(&left, crate::test_support::scenario_lineage(30));
    let right_second_strike = public_master_deck_ordinal_for_lineage_v1(
        &right,
        crate::test_support::scenario_lineage(30),
    );

    assert_eq!(left_bash, right_bash);
    assert_eq!(left_first_strike, right_first_strike);
    assert_eq!(left_second_strike, right_second_strike);
    assert_eq!(left_first_strike, left_second_strike);
    assert_ne!(left_bash, left_first_strike);
    assert_eq!(
        public_master_deck_ordinal_for_lineage_v1(
            &left,
            crate::test_support::scenario_lineage(999)
        ),
        None
    );
}

#[test]
fn draw_order_is_hidden_without_frozen_eye_and_visible_with_it() {
    let mut left = crate::test_support::blank_test_combat();
    left.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::Bash, 1),
        CombatCard::for_scenario(CardId::Defend, 2),
    ]
    .into();
    let mut right = left.clone();
    right.zones.draw_pile.swap(0, 1);

    assert_eq!(
        public_combat_state_v1(&left),
        public_combat_state_v1(&right)
    );

    left.entities
        .player
        .add_relic(RelicState::new(RelicId::FrozenEye));
    right
        .entities
        .player
        .add_relic(RelicState::new(RelicId::FrozenEye));
    assert_ne!(
        public_combat_state_v1(&left),
        public_combat_state_v1(&right)
    );
}

#[test]
fn identical_public_inputs_can_require_opposite_exact_combat_actions() {
    use crate::sim::combat::{
        CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
    };
    use crate::state::core::{ClientInput, EngineState};

    // A complete, tiny combat suffix: one energy and lethal incoming damage.
    // Pommel Strike + a drawn Anger kills; Havoc + a top-deck Bludgeon kills.
    // Reversing only the hidden pile therefore reverses the winning action.
    fn root() -> CombatPosition {
        use crate::content::monsters::factory::EncounterId;
        use crate::state::map::node::RoomType;

        let mut run = crate::test_support::natural_test_run(123, 0, false, "Ironclad");
        let (_, natural) = crate::sim::combat_start::build_natural_combat_start(
            &mut run,
            EncounterId::JawWorm,
            RoomType::MonsterRoom,
        )
        .unwrap();
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.player.current_hp = 1;
        combat.turn.energy = 1;
        combat.entities.monsters = natural.entities.monsters;
        assert_eq!(combat.entities.monsters[0].id, 1);
        combat.entities.monsters[0].current_hp = 12;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::PommelStrike, 1),
            CombatCard::for_scenario(CardId::Havoc, 2),
        ];
        // The engine stores this pile in draw order, top first.
        combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Anger, 4),
            CombatCard::for_scenario(CardId::Bludgeon, 3),
        ]
        .into();
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    fn outcomes(position: &CombatPosition, depth: usize) -> Vec<(ClientInput, bool)> {
        assert!(
            depth > 0,
            "this diagnostic must exhaust the whole combat tree"
        );
        let surface = EngineCombatStepper.legal_action_surface(position);
        assert!(surface.selection_families.is_empty());
        assert!(surface.indexed_choice.is_none());
        assert!(!surface.atomic_actions.is_empty());
        surface
            .atomic_actions
            .into_iter()
            .map(|action| {
                let next = EngineCombatStepper.apply_to_stable(
                    position,
                    action.clone(),
                    CombatStepLimits {
                        max_engine_steps: 512,
                        deadline: None,
                    },
                );
                assert!(!next.truncated && !next.timed_out);
                let wins = match next.terminal {
                    CombatTerminal::Win => true,
                    CombatTerminal::Loss => false,
                    CombatTerminal::Unresolved => outcomes(&next.position, depth - 1)
                        .into_iter()
                        .any(|(_, wins)| wins),
                };
                (action, wins)
            })
            .collect()
    }

    let anger = root();
    let mut bludgeon = anger.clone();
    bludgeon.combat.zones.draw_pile.swap(0, 1);
    assert_eq!(
        public_combat_state_v1(&anger.combat),
        public_combat_state_v1(&bludgeon.combat)
    );
    let pommel = ClientInput::PlayCard {
        card_index: 0,
        target: Some(1),
    };
    let havoc = ClientInput::PlayCard {
        card_index: 1,
        target: None,
    };
    for (position, expected) in [
        (&anger, [true, false, false]),
        (&bludgeon, [false, true, false]),
    ] {
        let results = outcomes(position, 4);
        assert_eq!(results.len(), 3);
        for (action, wins) in [&pommel, &havoc, &ClientInput::EndTurn]
            .into_iter()
            .zip(expected)
        {
            assert_eq!(
                results.iter().find(|(input, _)| input == action).unwrap().1,
                wins
            );
        }
    }
}

#[test]
fn runic_dome_hides_typed_intent_facts() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 7;
    combat.entities.monsters.push(monster);
    combat.set_monster_protocol_visible_intent(
        7,
        Intent::Attack {
            damage: 11,
            hits: 1,
        },
    );
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RunicDome));

    let observation = public_combat_state_v1(&combat);

    assert_eq!(observation.monsters[0].intent.intent, None);
    assert_eq!(
        observation.monsters[0].intent.hidden_reason,
        Some(HiddenInformationReasonV1::RunicDome)
    );
}

#[test]
fn public_state_keeps_domain_card_potion_and_enemy_identities() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Bash, 1)];
    combat.entities.potions = vec![Some(Potion::new(PotionId::FruitJuice, 41))];
    combat
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));

    let observation = public_combat_state_v1(&combat);

    assert_eq!(observation.cards.hand.cards[0].card_id, CardId::Bash);
    assert_eq!(
        observation.potions[0]
            .as_ref()
            .expect("owned potion")
            .potion_id,
        PotionId::FruitJuice
    );
    assert_eq!(
        observation.monsters[0].enemy,
        CombatLearningEnemyIdentityV1::Known {
            enemy_id: EnemyId::JawWorm,
        }
    );
}

#[test]
fn stasis_power_exposes_the_held_card_without_leaking_its_private_lineage() {
    let mut left = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::BronzeOrb);
    monster.id = 7;
    left.entities.monsters.push(monster);
    left.zones.limbo = vec![
        CombatCard::for_scenario(CardId::SecondWind, 41),
        CombatCard::for_scenario(CardId::BurningPact, 42),
    ];
    store::set_powers_for(
        &mut left,
        7,
        vec![crate::content::powers::core::stasis::power_holding_card(
            crate::test_support::scenario_lineage(41),
        )],
    );

    let mut right = left.clone();
    right.zones.limbo[0].lineage = crate::test_support::scenario_lineage(99);
    store::set_powers_for(
        &mut right,
        7,
        vec![crate::content::powers::core::stasis::power_holding_card(
            crate::test_support::scenario_lineage(99),
        )],
    );

    let left_observation = public_combat_state_v1(&left);
    let right_observation = public_combat_state_v1(&right);
    assert_eq!(left_observation, right_observation);
    assert_eq!(left_observation.cards.limbo.cards.len(), 1);
    assert_eq!(
        left_observation.cards.limbo.cards[0].card_id,
        CardId::BurningPact
    );
    let stasis = &left_observation.monsters[0].powers[0];
    assert_eq!(stasis.extra_data, 0);
    assert_eq!(
        stasis
            .payload_card
            .as_ref()
            .expect("visible stasis card")
            .card_id,
        CardId::SecondWind
    );

    let mut legacy = left_observation.clone();
    legacy.schema_version = 1;
    legacy.monsters[0].powers[0].extra_data = 41;
    legacy.monsters[0].powers[0].payload_card = None;
    assert!(public_combat_state_replay_compatible_v2(
        &legacy,
        &left_observation
    ));

    let mut different_state = left_observation.clone();
    different_state.player.hp -= 1;
    assert!(!public_combat_state_replay_compatible_v2(
        &legacy,
        &different_state
    ));
}

#[test]
fn hand_attack_projects_current_damage_for_each_monster_without_mutating_the_card() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut strike = CombatCard::for_scenario(CardId::Strike, 41);
    strike.base_damage_mut = 99;
    strike.base_block_mut = 88;
    strike.base_magic_num_mut = 77;
    strike.multi_damage = smallvec::smallvec![66];
    combat.zones.hand = vec![strike.clone()];

    let mut first = crate::test_support::test_monster(EnemyId::Cultist);
    first.id = 1;
    first.slot = 0;
    let mut second = crate::test_support::test_monster(EnemyId::JawWorm);
    second.id = 2;
    second.slot = 1;
    combat.entities.monsters = vec![first, second];
    store::set_powers_for(
        &mut combat,
        0,
        vec![
            test_power(PowerId::Strength, 1, 3),
            test_power(PowerId::Weak, 2, 1),
        ],
    );
    store::set_powers_for(&mut combat, 2, vec![test_power(PowerId::Vulnerable, 3, 1)]);

    let observation = public_combat_state_v1(&combat);
    let projected = &observation.cards.hand.cards[0];

    assert_eq!(projected.current_damage, 6);
    assert_eq!(projected.damage_by_monster_order, vec![6, 10]);
    assert_eq!(combat.zones.hand[0], strike);
}

#[test]
fn hand_multi_attack_projection_stays_aligned_to_monster_order() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::ThunderClap, 41)];

    let mut first = crate::test_support::test_monster(EnemyId::Cultist);
    first.id = 1;
    first.slot = 0;
    let mut second = crate::test_support::test_monster(EnemyId::JawWorm);
    second.id = 2;
    second.slot = 1;
    combat.entities.monsters = vec![first, second];
    store::set_powers_for(&mut combat, 2, vec![test_power(PowerId::Vulnerable, 1, 1)]);

    let observation = public_combat_state_v1(&combat);
    let projected = &observation.cards.hand.cards[0];

    assert_eq!(projected.current_damage, 4);
    assert_eq!(projected.damage_by_monster_order, vec![4, 6]);
}

#[test]
fn hand_skill_projects_current_block_without_fake_target_damage() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut defend = CombatCard::for_scenario(CardId::Defend, 41);
    defend.base_block_mut = 99;
    defend.multi_damage = smallvec::smallvec![55];
    combat.zones.hand = vec![defend.clone()];
    combat
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::Cultist));
    store::set_powers_for(&mut combat, 0, vec![test_power(PowerId::Dexterity, 1, 2)]);

    let observation = public_combat_state_v1(&combat);
    let projected = &observation.cards.hand.cards[0];

    assert_eq!(projected.current_block, 7);
    assert!(projected.damage_by_monster_order.is_empty());
    assert_eq!(combat.zones.hand[0], defend);
}

#[test]
fn dexterity_does_not_turn_non_block_cards_into_public_block_claims() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Shockwave, 41),
        CombatCard::for_scenario(CardId::Strike, 42),
        CombatCard::for_scenario(CardId::Defend, 43),
    ];
    combat
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::Cultist));
    store::set_powers_for(
        &mut combat,
        0,
        vec![
            test_power(PowerId::Dexterity, 1, 3),
            test_power(PowerId::Frail, 2, 5),
        ],
    );

    let observed = public_combat_state_v1(&combat);
    let blocks: Vec<_> = observed
        .cards
        .hand
        .cards
        .iter()
        .map(|card| card.current_block)
        .collect();
    assert_eq!(blocks, vec![0, 0, 6]);
    let effect = crate::agent::information::effect::public_combat_atomic_action_effect_v1(
        &observed,
        &crate::agent::information::action::PublicCombatAtomicActionV1::PlayCard {
            hand_index: 0,
            target_monster_index: None,
        },
    )
    .unwrap();
    let crate::agent::information::effect::PublicCombatCandidateEffectV1::PlayCard { card } =
        effect.effect
    else {
        panic!("Shockwave should project a card effect");
    };
    assert_eq!(card.displayed_block, 0);
    let raw_shockwave =
        crate::content::cards::evaluate_card_for_play(&combat.zones.hand[0], &combat, None);
    assert_eq!(raw_shockwave.base_block_mut, 2);
    let requests = crate::content::cards::resolve_card_play(
        CardId::Shockwave,
        &combat,
        &combat.zones.hand[0],
        None,
    );
    assert!(!requests.iter().any(|request| matches!(
        request.action,
        crate::runtime::action::Action::GainBlock { .. }
    )));
}

#[test]
fn serialized_public_state_uses_current_card_projection() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
    let observation = public_combat_state_v1(&combat);
    let value = serde_json::to_value(observation).expect("serialize public combat state");

    assert_eq!(value["schema_name"], "PublicCombatState");
    assert_eq!(value["schema_version"], PUBLIC_COMBAT_STATE_SCHEMA_VERSION);
    assert!(value.get("cards").is_some());
    assert!(value.get("player").is_some());
    assert!(value.get("player_summary").is_none());
    assert!(value.get("public_summary").is_none());
    let card = &value["cards"]["hand"]["cards"][0];
    assert_eq!(card["current_damage"], 6);
    assert!(card.get("base_damage_mut").is_none());
}

#[test]
fn public_powers_relics_and_dynamic_cards_change_public_state() {
    let mut baseline = crate::test_support::blank_test_combat();
    baseline.zones.hand = vec![CombatCard::for_scenario(CardId::Bash, 1)];
    let baseline_observation = public_combat_state_v1(&baseline);

    let mut changed = baseline.clone();
    changed.zones.hand[0].base_damage_mut = 4;
    changed
        .entities
        .player
        .add_relic(RelicState::new(RelicId::PenNib));
    let player_id = changed.entities.player.id;
    store::set_powers_for(
        &mut changed,
        player_id,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: Some(77),
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );

    assert_ne!(baseline_observation, public_combat_state_v1(&changed));
}

#[test]
fn private_monster_roll_state_does_not_change_public_state() {
    let mut left = crate::test_support::blank_test_combat();
    let mut louse = crate::test_support::test_monster(EnemyId::LouseNormal);
    louse.id = 7;
    louse.set_planned_move_id(1);
    louse.move_history_mut().push_back(1);
    louse.louse.bite_damage = Some(6);
    left.entities.monsters.push(louse);
    left.entities
        .player
        .add_relic(RelicState::new(RelicId::RunicDome));

    let mut right = left.clone();
    right.entities.monsters[0].set_planned_move_id(2);
    right.entities.monsters[0].move_history_mut().clear();
    right.entities.monsters[0].move_history_mut().push_back(2);
    right.entities.monsters[0].louse.bite_damage = Some(9);

    assert_eq!(
        public_combat_state_v1(&left),
        public_combat_state_v1(&right),
        "the private current roll and unrevealed louse damage must not leak"
    );
}

#[test]
fn executed_monster_history_is_public_but_the_current_roll_is_not() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 7;
    monster.set_planned_move_id(2);
    monster.move_history_mut().extend([1, 2]);
    combat.entities.monsters.push(monster);
    combat.record_monster_protocol_executed_move(7, 1);
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RunicDome));

    let observation = public_combat_state_v1(&combat);

    assert_eq!(
        observation.monsters[0].executed_moves,
        CombatLearningMonsterMoveHistoryV1 {
            evidence: ObservationEvidenceKindV1::PublicOrderedCollection,
            move_ids: vec![1],
        }
    );
}

#[test]
fn public_encounter_counters_change_public_state() {
    let mut thief_combat = crate::test_support::blank_test_combat();
    let mut looter = crate::test_support::test_monster(EnemyId::Looter);
    looter.thief.stolen_gold = 17;
    thief_combat.entities.monsters.push(looter);

    let thief_observation = public_combat_state_v1(&thief_combat);
    assert_eq!(
        thief_observation.monsters[0].public_counters,
        vec![CombatLearningMonsterPublicCounterV1::StolenGold { amount: 17 }]
    );

    let mut hexaghost_combat = crate::test_support::blank_test_combat();
    let mut hexaghost = crate::test_support::test_monster(EnemyId::Hexaghost);
    hexaghost.hexaghost.orb_active_count = 4;
    hexaghost_combat.entities.monsters.push(hexaghost);

    let hexaghost_observation = public_combat_state_v1(&hexaghost_combat);
    assert_eq!(
        hexaghost_observation.monsters[0].public_counters,
        vec![CombatLearningMonsterPublicCounterV1::HexaghostActiveOrbs { count: 4 }]
    );
}
