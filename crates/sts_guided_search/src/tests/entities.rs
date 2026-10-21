use super::*;
use crate::local::ConstructedCombat;
use crate::observation::{combat_observation, MonsterFactId};
use sts_core::content::monsters::factory::EncounterId;
use sts_core::state::map::node::RoomType;

fn root(encounter: EncounterId) -> CombatPosition {
    ConstructedCombat {
        seed: 73,
        encounter,
        act: 3,
        room: RoomType::MonsterRoomBoss,
        hp: 75,
        deck: vec![(CardId::Strike, 0); 8],
        relics: vec![],
        potions: vec![],
        max_potion_uses: Some(0),
    }
    .build()
    .unwrap()
}

fn encode(root: CombatPosition) -> crate::entities::EntityPosition {
    let session = SearchSession::new(
        CombatSearchSpace::default(),
        CombatSearchState::new(root).unwrap(),
        SearchConfig::default(),
        budget(),
    )
    .unwrap();
    crate::entities::encode(&session, session.root()).unwrap()
}

fn card_mechanics(
    encoded: &crate::entities::EntityPosition,
    id: CardId,
) -> &[f32; crate::entities::WIDTH] {
    let card = encoded
        .ids
        .iter()
        .position(|row| *row == [2, id.stable_ordinal() as i64])
        .unwrap();
    let mechanics = encoded
        .ids
        .iter()
        .enumerate()
        .position(|(row, kind)| *kind == [23, 0] && encoded.refs[row][0] == card as i64)
        .unwrap();
    &encoded.values[mechanics]
}

fn card_step(root: &CombatPosition, input: ClientInput) -> CombatPosition {
    assert!(EngineCombatStepper.is_legal_action(root, &input));
    let step = EngineCombatStepper.apply_to_stable(
        root,
        input,
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(!step.truncated && !step.timed_out);
    step.position
}

#[test]
fn power_relations_describe_future_strength_without_activating_it() {
    use sts_core::content::powers::mechanics::{PowerEffect, PowerLifetime, PowerTrigger};
    use sts_core::content::powers::store::power_amount;
    let mut r = root(EncounterId::JawWorm);
    let target = r.combat.entities.monsters[0].id;
    sts_core::engine::action_handlers::powers::handle_apply_power(
        target,
        target,
        PowerId::Anger,
        3,
        &mut r.combat,
    );
    r.combat.zones.hand = [CardId::Defend, CardId::Strike, CardId::Inflame]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, 20 + i as u32))
        .collect();
    let encoded = encode(r.clone());
    let active = encoded
        .ids
        .iter()
        .position(|id| *id == [4, PowerId::Anger.stable_ordinal() as i64])
        .unwrap();
    let relation = encoded
        .ids
        .iter()
        .enumerate()
        .find(|(row, id)| id[0] == 24 && encoded.refs[*row][0] == active as i64)
        .unwrap()
        .0;
    let owner = encoded.refs[active][0];
    assert_eq!(encoded.refs[relation][1], owner);
    let v = &encoded.values[relation];
    assert_eq!(v[0], 1.); // partial profile
    assert_eq!(v[1 + PowerTrigger::PlayerCardPlayed as usize], 1.);
    assert_eq!(v[6], 1.); // Skill
    assert_eq!(v[10 + PowerEffect::ApplyPower as usize], 1.);
    let strength = encoded.refs[relation][2] as usize;
    assert_eq!(
        encoded.ids[strength],
        [25, PowerId::Strength.stable_ordinal() as i64]
    );
    assert_eq!(
        encoded.values[strength][1 + PowerTrigger::AttackDamage as usize],
        1.
    );
    assert_eq!(
        encoded.values[strength][10 + PowerEffect::AddDamageDealt as usize],
        1.
    );
    assert_eq!(
        encoded.values[strength][22 + PowerLifetime::UntilChanged as usize],
        1.
    );
    assert!(!encoded
        .ids
        .iter()
        .any(|id| *id == [4, PowerId::Strength.stable_ordinal() as i64]));
    for (index, expected) in [(0, 3), (1, 0), (2, 0)] {
        let after = card_step(
            &r,
            ClientInput::PlayCard {
                card_index: index,
                target: (index == 1).then_some(target),
            },
        );
        assert_eq!(
            power_amount(&after.combat, target, PowerId::Strength),
            expected
        );
        // This relationship is attached to the power, even on a Jaw Worm.
        assert_eq!(
            sts_core::content::powers::calculate_monster_damage(5, target, 0, &after.combat),
            5 + expected
        );
    }
}

#[test]
fn planned_power_semantics_do_not_fire_before_the_power_is_applied() {
    use sts_core::content::powers::store::power_amount;
    let mut r = root(EncounterId::GremlinNob);
    let target = r.combat.entities.monsters[0].id;
    r.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 20)];
    let before = encode(r.clone());
    let planned = before
        .ids
        .iter()
        .position(|id| *id == [19, PowerId::Anger.stable_ordinal() as i64])
        .unwrap();
    assert!(before
        .ids
        .iter()
        .enumerate()
        .any(|(row, id)| id[0] == 24 && before.refs[row][0] == planned as i64));
    assert!(!before
        .ids
        .iter()
        .any(|id| *id == [4, PowerId::Anger.stable_ordinal() as i64]));
    let played = card_step(
        &r,
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
    );
    assert_eq!(power_amount(&played.combat, target, PowerId::Strength), 0);
    let next = card_step(&played, ClientInput::EndTurn);
    assert!(power_amount(&next.combat, target, PowerId::Anger) > 0);
    assert!(encode(next)
        .ids
        .iter()
        .any(|id| *id == [4, PowerId::Anger.stable_ordinal() as i64]));
}

#[test]
fn uncovered_power_is_explicit_and_shackled_schedule_survives_card_exhaust() {
    use sts_core::content::powers::mechanics::{PowerLifetime, PowerTrigger};
    let mut r = root(EncounterId::JawWorm);
    let target = r.combat.entities.monsters[0].id;
    sts_core::engine::action_handlers::powers::handle_apply_power(
        target,
        target,
        PowerId::Artifact,
        1,
        &mut r.combat,
    );
    let encoded = encode(r.clone());
    let artifact = encoded
        .ids
        .iter()
        .position(|id| *id == [4, PowerId::Artifact.stable_ordinal() as i64])
        .unwrap();
    let relation = encoded
        .ids
        .iter()
        .enumerate()
        .find(|(row, id)| id[0] == 24 && encoded.refs[*row][0] == artifact as i64)
        .unwrap()
        .0;
    assert_eq!(encoded.values[relation][0], 0.);
    assert_eq!(encoded.refs[relation][2], -1);
    // Remove Artifact before the actual debuff application.
    r.combat.entities.power_db.clear();
    r.combat.zones.hand = vec![CombatCard::for_scenario(CardId::DarkShackles, 20)];
    let after = card_step(
        &r,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(target),
        },
    );
    assert!(after.combat.zones.hand.is_empty());
    let encoded = encode(after);
    let shackled = encoded
        .ids
        .iter()
        .position(|id| *id == [4, PowerId::Shackled.stable_ordinal() as i64])
        .unwrap();
    let row = encoded
        .ids
        .iter()
        .enumerate()
        .find(|(row, id)| id[0] == 24 && encoded.refs[*row][0] == shackled as i64)
        .unwrap()
        .0;
    assert_eq!(
        encoded.values[row][1 + PowerTrigger::OwnerTurnEnd as usize],
        1.
    );
    assert_eq!(
        encoded.values[row][22 + PowerLifetime::OwnerTurnEnd as usize],
        1.
    );
    assert_eq!(encoded.values[row][26], 1.);
}

#[test]
fn card_mechanics_distinguish_persistent_and_scheduled_strength_loss() {
    use sts_core::content::cards::mechanics::CardMechanicRole;
    use sts_core::content::powers::store::power_amount;
    let mut r = root(EncounterId::JawWorm);
    r.combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Disarm, 20),
        CombatCard::for_scenario(CardId::DarkShackles, 21),
    ];
    r.combat.zones.hand[0].upgrades = 1;
    let encoded = encode(r.clone());
    let disarm = card_mechanics(&encoded, CardId::Disarm);
    let shackles = card_mechanics(&encoded, CardId::DarkShackles);
    assert_eq!(
        disarm[CardMechanicRole::PersistentEnemyAttackReduction as usize],
        1.
    );
    assert_eq!(disarm[24], 1.); // card-owned exhaust, before power overrides
    assert_eq!(&disarm[28..31], &[1., 1., 0.]);
    assert_eq!(disarm[31], -0.3);
    assert_eq!(&shackles[28..31], &[1., 0., 1.]);
    assert_eq!(shackles[31], -0.9);
    let target = r.combat.entities.monsters[0].id;
    for (index, amount, final_amount) in [(0, -3, -3), (1, -9, 0)] {
        let played = card_step(
            &r,
            ClientInput::PlayCard {
                card_index: index,
                target: Some(target),
            },
        );
        assert_eq!(
            power_amount(&played.combat, target, PowerId::Strength),
            amount
        );
        let next = card_step(&played, ClientInput::EndTurn);
        assert_eq!(
            power_amount(&next.combat, target, PowerId::Strength),
            final_amount
        );
        let after = encode(next);
        let owner = after
            .ids
            .iter()
            .position(|kind| *kind == [3, EnemyId::JawWorm.stable_ordinal() as i64])
            .unwrap();
        let strength = after.ids.iter().enumerate().find(|(row, kind)| {
            **kind == [4, PowerId::Strength.stable_ordinal() as i64]
                && after.refs[*row][0] == owner as i64
        });
        if final_amount != 0 {
            assert_eq!(
                after.values[strength.unwrap().0][1],
                final_amount as f32 / 10.
            );
        }
    }
}

#[test]
fn card_mechanics_expose_shared_draw_requests_without_promising_draw_results() {
    use sts_core::content::cards::mechanics::CardMechanicRole;
    let mut r = root(EncounterId::JawWorm);
    r.combat.zones.hand = [
        CardId::BattleTrance,
        CardId::ShrugItOff,
        CardId::PommelStrike,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 20))
    .collect();
    r.combat.zones.hand[2].upgrades = 1;
    let encoded = encode(r.clone());
    for (id, draws) in [
        (CardId::BattleTrance, 3),
        (CardId::ShrugItOff, 1),
        (CardId::PommelStrike, 2),
    ] {
        let mechanics = card_mechanics(&encoded, id);
        assert_eq!(mechanics[CardMechanicRole::DrawCards as usize], 1.);
        assert_eq!(&mechanics[26..28], &[1., draws as f32 / 5.]);
        let card = encoded
            .ids
            .iter()
            .position(|kind| *kind == [2, id.stable_ordinal() as i64])
            .unwrap();
        let type_column = if id == CardId::PommelStrike { 27 } else { 28 };
        assert_eq!(encoded.values[card][type_column], 1.);
        assert_eq!(encoded.values[card][27..32].iter().sum::<f32>(), 1.);
    }
    assert_eq!(
        card_mechanics(&encoded, CardId::BattleTrance)
            [CardMechanicRole::PreventFurtherDrawThisTurn as usize],
        1.
    );
    let played = card_step(
        &r,
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
    );
    let after = encode(played.clone());
    assert_eq!(
        &card_mechanics(&after, CardId::ShrugItOff)[26..28],
        &[1., 0.2]
    );
    assert!(after
        .ids
        .iter()
        .any(|kind| *kind == [4, PowerId::NoDraw.stable_ordinal() as i64]));
    let shrug = played
        .combat
        .zones
        .hand
        .iter()
        .position(|c| c.id == CardId::ShrugItOff)
        .unwrap();
    let after_shrug = card_step(
        &played,
        ClientInput::PlayCard {
            card_index: shrug,
            target: None,
        },
    );
    assert_eq!(
        after_shrug.combat.zones.hand.len(),
        played.combat.zones.hand.len() - 1
    );
}

#[test]
fn potion_facts_share_roles_and_effective_amounts_with_actual_use() {
    use sts_core::content::potions::PotionMechanicRole;
    let mut r = root(EncounterId::JawWorm);
    r.combat
        .entities
        .player
        .add_relic(sts_core::content::relics::RelicState::new(
            sts_core::content::relics::RelicId::SacredBark,
        ));
    r.combat.entities.potions = vec![
        Some(Potion::new(PotionId::BlockPotion, 31)),
        Some(Potion::new(PotionId::SpeedPotion, 32)),
        Some(Potion::new(PotionId::DexterityPotion, 33)),
        None,
    ];
    let encoded = encode(r.clone());
    let slots: Vec<_> = encoded
        .ids
        .iter()
        .enumerate()
        .filter(|(_, id)| id[0] == 6)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(encoded.values[slots[0]][2], 24. / 20.);
    assert_eq!(encoded.values[slots[0]][3], 12. / 20.);
    assert_eq!(encoded.values[slots[0]][4], 1.);
    let dexterity: Vec<_> = encoded
        .ids
        .iter()
        .enumerate()
        .filter(|(_, id)| **id == [22, PotionMechanicRole::GainDexterity as i64])
        .map(|(i, _)| encoded.refs[i][0])
        .collect();
    assert_eq!(dexterity, [slots[1] as i64, slots[2] as i64]);
    assert!(encoded
        .ids
        .iter()
        .enumerate()
        .any(|(i, id)| *id == [22, PotionMechanicRole::Temporary as i64]
            && encoded.refs[i][0] == slots[1] as i64));
    assert!(!encoded
        .ids
        .iter()
        .enumerate()
        .any(|(i, id)| id[0] == 22 && encoded.refs[i][0] == slots[3] as i64));
    assert!(encoded.values[slots[3]][1..].iter().all(|v| *v == 0.));
    let step = EngineCombatStepper.apply_to_stable(
        &r,
        ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(!step.truncated && !step.timed_out);
    assert_eq!(
        step.position.combat.entities.player.block - r.combat.entities.player.block,
        24
    );
    let used = encode(step.position);
    assert_eq!(
        used.ids
            .iter()
            .filter(|id| **id == [22, PotionMechanicRole::GainBlock as i64])
            .count(),
        0
    );
}

#[test]
fn discovery_potion_inputs_distinguish_copies_offers_cost_and_skip() {
    let mut r = root(EncounterId::JawWorm);
    r.combat
        .entities
        .player
        .add_relic(sts_core::content::relics::RelicState::new(
            sts_core::content::relics::RelicId::SacredBark,
        ));
    r.combat.entities.potions = vec![
        Some(Potion::new(PotionId::PowerPotion, 31)),
        Some(Potion::new(PotionId::ColorlessPotion, 32)),
    ];
    let encoded = encode(r.clone());
    let slots: Vec<_> = encoded
        .ids
        .iter()
        .enumerate()
        .filter(|(_, id)| id[0] == 6)
        .map(|(i, _)| i)
        .collect();
    for &row in &slots {
        assert_eq!(encoded.values[row][8], 1.);
        assert_eq!(encoded.values[row][12], 2.); // Bark doubles selected copies
        assert_eq!(encoded.values[row][13], 1.); // present zero cost
        assert_eq!(encoded.values[row][14], 0.);
        assert_eq!(encoded.values[row][16], 3.); // still three offered cards
    }
    assert_eq!(encoded.values[slots[0]][15], 1.);
    assert_eq!(encoded.values[slots[1]][15], 0.);
    let limits = CombatStepLimits {
        max_engine_steps: 512,
        deadline: None,
    };
    let offered = EngineCombatStepper.apply_to_stable(
        &r,
        ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
        limits,
    );
    assert!(!offered.truncated && !offered.timed_out);
    let described = encode(offered.position.clone());
    for (row, _) in described
        .ids
        .iter()
        .enumerate()
        .filter(|(_, kind)| kind[0] == 9)
    {
        assert_eq!(described.values[row][27..32].iter().sum::<f32>(), 1.);
        let mechanics = described
            .ids
            .iter()
            .enumerate()
            .find(|(i, kind)| **kind == [23, 0] && described.refs[*i][0] == row as i64)
            .unwrap()
            .0;
        assert_eq!(described.values[mechanics][26], 0.); // no live draw number yet
        assert_eq!(described.values[mechanics][28], 0.); // no live Strength magnitude yet
    }
    let surface = EngineCombatStepper.legal_action_surface(&offered.position);
    assert_eq!(surface.indexed_choice.as_ref().unwrap().candidates.len(), 3);
    let chosen = EngineCombatStepper.apply_to_stable(
        &offered.position,
        ClientInput::SubmitDiscoverChoice(0),
        limits,
    );
    assert!(!chosen.truncated && !chosen.timed_out);
    assert_eq!(
        chosen.position.combat.zones.hand.len(),
        r.combat.zones.hand.len() + 2
    );
}

#[test]
fn collector_tracks_current_torches_instead_of_stale_same_name_entities() {
    use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
    let initial = root(EncounterId::Collector);
    let step = EngineCombatStepper.apply_to_stable(
        &initial,
        ClientInput::EndTurn,
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(!step.truncated && !step.timed_out);
    let mut r = step.position;
    let mut stale = r
        .combat
        .entities
        .monsters
        .iter()
        .find(|m| m.enemy_id == EnemyId::TorchHead)
        .unwrap()
        .clone();
    stale.id = 999;
    stale.current_hp = 0;
    r.combat.entities.monsters.insert(0, stale);
    let collector_index = r
        .combat
        .entities
        .monsters
        .iter()
        .position(|m| m.enemy_id == EnemyId::TheCollector)
        .unwrap();
    let targets = r.combat.entities.monsters[collector_index]
        .collector
        .enemy_slots;
    let expected = targets.map(|id| {
        r.combat
            .entities
            .monsters
            .iter()
            .position(|m| Some(m.id) == id)
            .unwrap()
    });
    let p = encode(r.clone());
    let monster_rows: Vec<_> = p
        .ids
        .iter()
        .enumerate()
        .filter(|(_, id)| id[0] == 3)
        .map(|(i, _)| i)
        .collect();
    let rows: Vec<_> = p
        .ids
        .iter()
        .enumerate()
        .filter(|(_, id)| id[0] == 14 && id[1] == MonsterFactId::TrackedMinionSlot as i64)
        .collect();
    assert_eq!(rows.len(), 2);
    for (slot, (i, _)) in rows.into_iter().enumerate() {
        assert_eq!(p.values[i][0], slot as f32);
        assert_eq!(p.refs[i][0], monster_rows[collector_index] as i64);
        assert_eq!(p.refs[i][1], monster_rows[expected[slot]] as i64);
        assert_ne!(p.refs[i][1], monster_rows[0] as i64);
    }
    assert!(p
        .ids
        .iter()
        .any(|id| *id == [14, MonsterFactId::TurnsTaken as i64]));
    r.combat.entities.monsters[collector_index]
        .collector
        .enemy_slots[0] = Some(888);
    assert!(crate::entities::encode_position(r, 0, None)
        .unwrap_err()
        .contains("enemy decision state outside entity task"));
}

#[test]
fn guardian_intent_has_specific_ordered_debuffs_and_targets() {
    let mut r = root(EncounterId::TheGuardian);
    r.combat.entities.monsters[0].set_planned_move_id(7); // Vent Steam
    let p = encode(r);
    let rows: Vec<_> = p
        .ids
        .iter()
        .enumerate()
        .filter(|(_, id)| id[0] == 19)
        .collect();
    assert_eq!(rows.len(), 2);
    for ((i, id), power) in rows.into_iter().zip([PowerId::Weak, PowerId::Vulnerable]) {
        assert_eq!(id[1], power.stable_ordinal() as i64);
        assert_eq!(p.refs[i][1], 0); // player, not the source monster
        assert_eq!(p.ids[p.refs[i][0] as usize][0], 3);
        assert_eq!(p.values[i][4], 0.2); // two layers, not generic nonattack
        assert_eq!(p.values[i][5], 1.); // applies a future power
    }
    let step_rows: Vec<_> = p
        .ids
        .iter()
        .enumerate()
        .filter(|(_, id)| id[0] == 19)
        .map(|(i, _)| p.values[i][0])
        .collect();
    assert_eq!(step_rows, [0., 0.125]);
}

#[test]
fn haste_availability_distinguishes_equal_current_intents_and_actual_next_moves() {
    let mut ready = root(EncounterId::TimeEater);
    let m = &mut ready.combat.entities.monsters[0];
    m.current_hp = m.max_hp / 2 - 1;
    let mut spent = ready.clone();
    spent.combat.entities.monsters[0].time_eater.used_haste = true;
    let a = combat_observation(&CombatSearchState::new(ready.clone()).unwrap()).unwrap();
    let b = combat_observation(&CombatSearchState::new(spent.clone()).unwrap()).unwrap();
    assert_eq!(a.monsters[0].intent, b.monsters[0].intent);
    assert_eq!(a.monsters[0].hp, b.monsters[0].hp);
    assert_ne!(encode(ready.clone()), encode(spent.clone()));
    let mut next_moves = vec![];
    for r in [ready, spent] {
        let next = EngineCombatStepper.apply_to_stable(
            &r,
            ClientInput::EndTurn,
            CombatStepLimits {
                max_engine_steps: 256,
                deadline: None,
            },
        );
        assert!(!next.truncated && !next.timed_out);
        let m = &next.position.combat.entities.monsters[0];
        next_moves.push(m.planned_move_id());
        let o =
            combat_observation(&CombatSearchState::new(next.position.clone()).unwrap()).unwrap();
        let d = o.monsters[0].decision.as_ref().unwrap();
        assert_eq!(d.planned_move, m.planned_move_id());
        assert_eq!(
            d.roll_history,
            m.move_history().iter().copied().collect::<Vec<_>>()
        );
        encode(next.position);
    }
    assert_eq!(next_moves[0], 5); // Time Eater's actual Haste move
    assert_ne!(next_moves[1], 5);
}

#[test]
fn controller_facts_keep_guardian_progress_and_roll_order_without_protocol_identity() {
    let mut r = root(EncounterId::TheGuardian);
    r.combat.entities.monsters[0].guardian.damage_taken = 7;
    let state = CombatSearchState::new(r.clone()).unwrap();
    let obs = combat_observation(&state).unwrap();
    let decision = obs.monsters[0].decision.as_ref().unwrap();
    assert!(decision
        .facts
        .iter()
        .any(|f| f.fact == MonsterFactId::DamageTaken && f.value == 7));
    let p = encode(r.clone());
    let fact = p
        .ids
        .iter()
        .position(|row| *row == [14, MonsterFactId::DamageTaken as i64])
        .unwrap();
    assert_eq!(p.values[fact][0], 7. / 50.);
    assert_eq!(p.ids[p.refs[fact][0] as usize][0], 3);
    r.combat.entities.monsters[0]
        .move_history_mut()
        .extend([2, 7]);
    let a = encode(r.clone());
    r.combat.entities.monsters[0].move_history_mut().pop_back();
    r.combat.entities.monsters[0].move_history_mut().pop_back();
    r.combat.entities.monsters[0]
        .move_history_mut()
        .extend([7, 2]);
    assert_ne!(a, encode(r));
    assert!(!serde_json::to_string(&decision)
        .unwrap()
        .contains("protocol_seeded"));
}

fn bottled_root() -> CombatPosition {
    use sts_core::content::relics::RelicState;
    use sts_core::runtime::combat::CardLineageId;
    let mut r = root(EncounterId::TheGuardian);
    let mut bound = CombatCard::for_scenario(CardId::Corruption, 7);
    bound.lineage = CardLineageId::master_deck(7);
    let mut deck = r.combat.meta.master_deck_snapshot.to_vec();
    deck.push(bound.clone());
    r.combat.meta.master_deck_snapshot = deck.into();
    r.combat.zones.hand.push(bound);
    // Explicit fixture payload; production resolves it exclusively via the relic owner.
    let bottle: RelicState = serde_json::from_str(r#"{"id":"BottledTornado","amount":7}"#).unwrap();
    r.combat.entities.player.relics.push(bottle);
    r.combat.entities.player.max_hp = 109;
    r.combat.entities.player.current_hp = 84;
    r
}

#[test]
fn exact_entity_entry_keeps_hp_counters_bindings_and_legal_actions() {
    let r = bottled_root();
    let before = r.clone();
    let p = crate::entities::encode_position(r.clone(), 0, None).unwrap();
    assert_eq!(p, encode(r));
    assert_eq!(before.combat.entities.player.current_hp, 84);
    assert_eq!(p.values[0][0], 84. / 75.);
    assert_eq!(p.values[0][1], 109. / 75.);
    let bottle = p
        .ids
        .iter()
        .position(|id| {
            *id == [
                5,
                sts_core::content::relics::RelicId::BottledTornado.stable_ordinal() as i64,
            ]
        })
        .unwrap();
    assert_eq!(
        p.values[bottle][3], 0.,
        "no lineage magnitude in model facts"
    );
    let description = p.refs[bottle][1] as usize;
    assert_eq!(
        p.ids[description],
        [2, CardId::Corruption.stable_ordinal() as i64]
    );
    assert_eq!(
        p.values[description][5], 1.,
        "described card, not another live card"
    );
    let live = p.refs[description][1] as usize;
    assert_eq!(p.ids[live], p.ids[description]);
    assert_eq!(p.values[live][0], 1., "actual bound card is in hand");
    assert!(p.action_refs.iter().any(|refs| refs[0] == live as i64));

    let mut renumbered = before.clone();
    let new = sts_core::runtime::combat::CardLineageId::master_deck(83);
    renumbered
        .combat
        .meta
        .master_deck_snapshot
        .with_cards_mut(|cards| cards.last_mut().unwrap().lineage = new);
    renumbered.combat.zones.hand.last_mut().unwrap().lineage = new;
    renumbered
        .combat
        .entities
        .player
        .relics
        .last_mut()
        .unwrap()
        .amount = 83;
    renumbered.combat.rng.shuffle_rng.random_long();
    assert_eq!(
        p,
        crate::entities::encode_position(renumbered, 0, None).unwrap()
    );

    let limited = crate::entities::encode_position(before.clone(), 1, Some(1)).unwrap();
    assert_eq!(limited.values[0][8], 1.);
    assert!(!limited.action_ids.iter().any(|id| id[0] == 3));
    assert!(crate::entities::encode_position(before, 2, Some(1)).is_err());
}

#[test]
fn bottle_description_survives_played_power_without_inventing_a_live_card() {
    let r = bottled_root();
    let card_index = r.combat.zones.hand.len() - 1;
    let next = EngineCombatStepper.apply_to_stable(
        &r,
        ClientInput::PlayCard {
            card_index,
            target: None,
        },
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(!next.timed_out && !next.truncated);
    let p = crate::entities::encode_position(next.position, 0, None).unwrap();
    let descriptions: Vec<_> = p
        .ids
        .iter()
        .enumerate()
        .filter(|(i, id)| {
            **id == [2, CardId::Corruption.stable_ordinal() as i64] && p.values[*i][5] == 1.
        })
        .collect();
    assert_eq!(descriptions.len(), 1);
    assert_eq!(p.refs[descriptions[0].0][1], -1);
}

#[test]
fn exact_entity_entry_rejects_broken_bottle_bindings_instead_of_guessing_by_name() {
    let mut r = bottled_root();
    r.combat
        .meta
        .master_deck_snapshot
        .remove_card_by_lineage(sts_core::runtime::combat::CardLineageId::master_deck(7));
    assert!(crate::entities::encode_position(r, 0, None)
        .unwrap_err()
        .contains("bottled target missing"));
    let mut r = bottled_root();
    r.combat
        .zones
        .discard_pile
        .push(r.combat.zones.hand.last().unwrap().clone());
    assert!(crate::entities::encode_position(r, 0, None)
        .unwrap_err()
        .contains("ambiguous bottled live-card"));
}
