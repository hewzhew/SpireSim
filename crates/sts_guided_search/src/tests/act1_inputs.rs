//! Input admission and real successor checks, not policy-quality measurements.
use super::*;
use crate::local::ConstructedCombat;
use crate::observation::{combat_observation, MonsterFactId};
use sts_core::content::monsters::encounter_pool::{public_encounter_pool, EncounterPoolTier};
use sts_core::content::monsters::factory::EncounterId;
use sts_core::sim::combat::CombatTerminal;
use sts_core::state::map::node::RoomType;

fn root(seed: u64, encounter: EncounterId, room: RoomType) -> CombatPosition {
    let mut deck = vec![(CardId::Strike, 0); 5];
    deck.extend(vec![(CardId::Defend, 0); 4]);
    deck.extend([
        (CardId::Bash, 0),
        (CardId::AscendersBane, 0),
        (CardId::Bludgeon, 0),
    ]);
    ConstructedCombat {
        seed,
        encounter,
        act: 1,
        room,
        hp: 68,
        deck,
        relics: vec![],
        potions: vec![PotionId::FirePotion],
        max_potion_uses: None,
    }
    .build()
    .unwrap()
}

fn encode(root: &CombatPosition) -> crate::entities::EntityPosition {
    let session = SearchSession::new(
        CombatSearchSpace::default(),
        CombatSearchState::new(root.clone()).unwrap(),
        SearchConfig::default(),
        budget(),
    )
    .unwrap();
    crate::entities::encode(&session, session.root()).unwrap()
}

fn step(root: &CombatPosition, input: ClientInput) -> CombatPosition {
    assert!(EngineCombatStepper.is_legal_action(root, &input));
    let next = EngineCombatStepper.apply_to_stable(
        root,
        input,
        CombatStepLimits {
            max_engine_steps: 256,
            deadline: None,
        },
    );
    assert!(!next.truncated && !next.timed_out);
    next.position
}

fn fact(root: &CombatPosition, monster: usize, id: MonsterFactId) -> i32 {
    let obs = combat_observation(&CombatSearchState::new(root.clone()).unwrap()).unwrap();
    obs.monsters[monster]
        .decision
        .as_ref()
        .unwrap()
        .facts
        .iter()
        .find(|f| f.fact == id)
        .unwrap()
        .value
}

#[test]
fn all_act1_encounter_pools_admit_openings_and_actual_turn_prefixes() {
    let mut seen = std::collections::HashSet::new();
    for tier in [
        EncounterPoolTier::Weak,
        EncounterPoolTier::Strong,
        EncounterPoolTier::Elite,
    ] {
        for encounter in public_encounter_pool(1, tier) {
            for seed in 0..32 {
                let room = if tier == EncounterPoolTier::Elite {
                    RoomType::MonsterRoomElite
                } else {
                    RoomType::MonsterRoom
                };
                let initial = root(seed, encounter.encounter, room);
                let mut state = initial.clone();
                let mut inputs = vec![];
                for _ in 0..4 {
                    seen.extend(state.combat.entities.monsters.iter().map(|m| m.enemy_id));
                    if EngineCombatStepper.terminal(&state) != CombatTerminal::Unresolved {
                        break;
                    }
                    encode(&state);
                    state = step(&state, ClientInput::EndTurn);
                    inputs.push(ClientInput::EndTurn);
                }
                if EngineCombatStepper.terminal(&state) == CombatTerminal::Unresolved {
                    encode(&state);
                }
                let replay = inputs.into_iter().fold(initial, |s, input| step(&s, input));
                assert_eq!(
                    replay, state,
                    "{tier:?} {:?} seed {seed}",
                    encounter.encounter
                );
            }
        }
    }
    for enemy in [
        EnemyId::JawWorm,
        EnemyId::Cultist,
        EnemyId::LouseNormal,
        EnemyId::LouseDefensive,
        EnemyId::AcidSlimeL,
        EnemyId::AcidSlimeM,
        EnemyId::AcidSlimeS,
        EnemyId::SpikeSlimeL,
        EnemyId::SpikeSlimeM,
        EnemyId::SpikeSlimeS,
        EnemyId::SlaverRed,
        EnemyId::SlaverBlue,
        EnemyId::Looter,
        EnemyId::FungiBeast,
        EnemyId::GremlinFat,
        EnemyId::GremlinThief,
        EnemyId::GremlinWarrior,
        EnemyId::GremlinWizard,
        EnemyId::GremlinTsundere,
        EnemyId::GremlinNob,
        EnemyId::Lagavulin,
        EnemyId::Sentry,
    ] {
        assert!(seen.contains(&enemy), "unexercised Act 1 enemy {enemy:?}");
    }
}

#[test]
fn all_act1_bosses_admit_openings_and_actual_turn_prefixes() {
    for encounter in [
        EncounterId::SlimeBoss,
        EncounterId::TheGuardian,
        EncounterId::Hexaghost,
    ] {
        for seed in 0..8 {
            let mut state = root(seed, encounter, RoomType::MonsterRoomBoss);
            for _ in 0..4 {
                if EngineCombatStepper.terminal(&state) != CombatTerminal::Unresolved {
                    break;
                }
                encode(&state);
                state = step(&state, ClientInput::EndTurn);
            }
        }
    }
}

#[test]
fn slime_boss_split_keeps_dead_parent_and_both_children_encodable() {
    let initial = ConstructedCombat {
        seed: 37,
        encounter: EncounterId::SlimeBoss,
        act: 1,
        room: RoomType::MonsterRoomBoss,
        hp: 75,
        deck: vec![(CardId::Bludgeon, 1); 5],
        relics: vec![],
        potions: vec![],
        max_potion_uses: Some(0),
    }
    .build()
    .unwrap();
    let mut state = initial.clone();
    let mut inherited_hp = 0;
    for _ in 0..2 {
        let target = state.combat.entities.monsters[0].id;
        state = step(
            &state,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
        );
        encode(&state);
        inherited_hp = state.combat.entities.monsters[0].current_hp;
        state = step(&state, ClientInput::EndTurn);
        encode(&state);
    }
    let parent = state
        .combat
        .entities
        .monsters
        .iter()
        .find(|m| m.enemy_id == EnemyId::SlimeBoss)
        .unwrap();
    assert_eq!(parent.current_hp, 0);
    let children: Vec<_> = state
        .combat
        .entities
        .monsters
        .iter()
        .filter(|m| m.current_hp > 0 && !m.is_dying)
        .collect();
    assert_eq!(children.len(), 2);
    assert!(children.iter().all(|m| m.current_hp == inherited_hp));
    assert!(children.iter().any(|m| m.enemy_id == EnemyId::AcidSlimeL));
    assert!(children.iter().any(|m| m.enemy_id == EnemyId::SpikeSlimeL));
    let split = encode(&state);
    assert!(split
        .refs
        .iter()
        .flatten()
        .all(|r| *r >= -1 && *r < split.ids.len() as i64));
    let mut unseeded = initial;
    unseeded.combat.entities.monsters[0]
        .slime_boss
        .protocol_seeded = false;
    assert!(crate::entities::encode_position(unseeded, 0, Some(0))
        .unwrap_err()
        .contains("enemy decision state"));
}

#[test]
fn hexaghost_controller_and_typed_burn_effects_survive_full_cycle() {
    let mut state = ConstructedCombat {
        seed: 37,
        encounter: EncounterId::Hexaghost,
        act: 1,
        room: RoomType::MonsterRoomBoss,
        hp: 75,
        deck: vec![(CardId::Defend, 1); 10],
        relics: vec![],
        potions: vec![],
        max_potion_uses: Some(0),
    }
    .build()
    .unwrap();
    assert_eq!(fact(&state, 0, MonsterFactId::Activated), 1);
    assert_eq!(fact(&state, 0, MonsterFactId::OrbActiveCount), 0);
    let mut saw_burn_upgrade = false;
    for turn in 0..10 {
        encode(&state);
        for _ in 0..3 {
            let Some(card_index) = state
                .combat
                .zones
                .hand
                .iter()
                .position(|c| c.id == CardId::Defend)
            else {
                break;
            };
            state = step(
                &state,
                ClientInput::PlayCard {
                    card_index,
                    target: None,
                },
            );
        }
        state = step(&state, ClientInput::EndTurn);
        assert_eq!(
            EngineCombatStepper.terminal(&state),
            CombatTerminal::Unresolved
        );
        let input = encode(&state);
        if turn == 0 {
            assert_eq!(fact(&state, 0, MonsterFactId::DividerDamage), 7);
        }
        if fact(&state, 0, MonsterFactId::BurnUpgraded) == 1 {
            saw_burn_upgrade = true;
            if let Some(row) = input
                .ids
                .iter()
                .position(|id| *id == [20, CardId::Burn.stable_ordinal() as i64])
            {
                assert_eq!(input.values[row][5], 1., "later Sear adds upgraded Burns");
            }
        }
    }
    assert!(saw_burn_upgrade);
    let observation = combat_observation(&CombatSearchState::new(state).unwrap()).unwrap();
    assert!(
        !observation.monsters[0]
            .decision
            .as_ref()
            .unwrap()
            .facts
            .iter()
            .any(|f| f.fact == MonsterFactId::DividerDamage),
        "the locked Divider damage is cleared after executing Divider"
    );
}

#[test]
fn lagavulin_equal_sleep_intents_have_distinct_remaining_setup_time() {
    let early = root(73, EncounterId::Lagavulin, RoomType::MonsterRoomElite);
    let late = step(&step(&early, ClientInput::EndTurn), ClientInput::EndTurn);
    assert_eq!(
        early.combat.entities.monsters[0].planned_move_id(),
        late.combat.entities.monsters[0].planned_move_id()
    );
    assert_eq!(fact(&early, 0, MonsterFactId::IdleCount), 0);
    assert_eq!(fact(&late, 0, MonsterFactId::IdleCount), 2);
    assert_ne!(encode(&early), encode(&late));
    let still_asleep = step(&early, ClientInput::EndTurn);
    let awake = step(&late, ClientInput::EndTurn);
    assert_eq!(fact(&still_asleep, 0, MonsterFactId::Awake), 0);
    assert_eq!(fact(&awake, 0, MonsterFactId::Awake), 1);
    assert_ne!(
        still_asleep.combat.entities.monsters[0].planned_move_id(),
        awake.combat.entities.monsters[0].planned_move_id()
    );
    encode(&awake);
}

#[test]
fn bludgeon_wakes_lagavulin_and_tracks_its_attack_debuff_cycle() {
    let mut spec = ConstructedCombat {
        seed: 3,
        encounter: EncounterId::Lagavulin,
        act: 1,
        room: RoomType::MonsterRoomElite,
        hp: 75,
        deck: vec![(CardId::Bludgeon, 0); 5],
        relics: vec![],
        potions: vec![],
        max_potion_uses: Some(0),
    };
    let initial = spec.build().unwrap();
    let target = initial.combat.entities.monsters[0].id;
    let awake = step(
        &initial,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(target),
        },
    );
    assert_eq!(fact(&awake, 0, MonsterFactId::Awake), 1);
    assert_eq!(fact(&awake, 0, MonsterFactId::WakeTriggered), 1);
    let mut state = step(&awake, ClientInput::EndTurn); // interrupted awakening turn
    for attacks in [1, 2, 0] {
        state = step(&state, ClientInput::EndTurn);
        assert_eq!(fact(&state, 0, MonsterFactId::DebuffTurnCount), attacks);
        encode(&state);
    }
    let obs = combat_observation(&CombatSearchState::new(state).unwrap()).unwrap();
    assert!(obs
        .powers
        .iter()
        .any(|p| p.power == PowerId::Strength && p.amount == Some(-2)));
    assert!(obs
        .powers
        .iter()
        .any(|p| p.power == PowerId::Dexterity && p.amount == Some(-2)));
    // Damage fully absorbed by sleeping block must not wake Lagavulin.
    spec.deck = vec![(CardId::Strike, 0); 5];
    let sleeping = spec.build().unwrap();
    let target = sleeping.combat.entities.monsters[0].id;
    let blocked = step(
        &sleeping,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(target),
        },
    );
    assert_eq!(fact(&blocked, 0, MonsterFactId::Awake), 0);
}

#[test]
fn looter_counters_preserve_theft_and_the_followup_change() {
    let initial = root(73, EncounterId::Looter, RoomType::MonsterRoom);
    let once = step(&initial, ClientInput::EndTurn);
    let twice = step(&once, ClientInput::EndTurn);
    assert_eq!(fact(&initial, 0, MonsterFactId::SlashCount), 0);
    assert_eq!(fact(&once, 0, MonsterFactId::SlashCount), 1);
    assert_eq!(fact(&twice, 0, MonsterFactId::SlashCount), 2);
    assert_eq!(fact(&once, 0, MonsterFactId::StolenGold), 20);
    assert_eq!(fact(&twice, 0, MonsterFactId::StolenGold), 40);
    assert_ne!(
        once.combat.entities.monsters[0].planned_move_id(),
        twice.combat.entities.monsters[0].planned_move_id()
    );
    encode(&twice);
}

#[test]
fn large_slime_split_and_generated_children_keep_valid_numeric_references() {
    let mut colors = std::collections::HashSet::new();
    for seed in 0..16 {
        let initial = ConstructedCombat {
            seed,
            encounter: EncounterId::LargeSlime,
            act: 1,
            room: RoomType::MonsterRoom,
            hp: 75,
            deck: vec![(CardId::Bludgeon, 1); 5],
            relics: vec![],
            potions: vec![],
            max_potion_uses: Some(0),
        }
        .build()
        .unwrap();
        colors.insert(initial.combat.entities.monsters[0].enemy_id);
        let target = initial.combat.entities.monsters[0].id;
        let ready = step(
            &initial,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
        );
        assert_eq!(fact(&ready, 0, MonsterFactId::SplitTriggered), 1);
        encode(&ready);
        let split = step(&ready, ClientInput::EndTurn);
        let alive: Vec<_> = split
            .combat
            .entities
            .monsters
            .iter()
            .filter(|m| m.current_hp > 0 && !m.is_dying)
            .collect();
        assert_eq!(alive.len(), 2);
        assert!(alive
            .iter()
            .all(|m| matches!(m.enemy_id, EnemyId::AcidSlimeM | EnemyId::SpikeSlimeM)));
        let input = encode(&split);
        assert!(input
            .refs
            .iter()
            .flatten()
            .all(|r| *r >= -1 && *r < input.ids.len() as i64));
        assert!(input
            .action_refs
            .iter()
            .flatten()
            .all(|r| *r >= -1 && *r < input.ids.len() as i64));
    }
    assert_eq!(colors.len(), 2);
}

#[test]
fn uninspected_controllers_are_still_rejected() {
    let r = root(73, EncounterId::GremlinLeader, RoomType::MonsterRoomElite);
    let session = SearchSession::new(
        CombatSearchSpace::default(),
        CombatSearchState::new(r).unwrap(),
        SearchConfig::default(),
        budget(),
    )
    .unwrap();
    assert!(crate::entities::encode(&session, session.root())
        .unwrap_err()
        .contains("enemy decision state"));
}
