use std::collections::VecDeque;

use crate::content::monsters::factory::{self, EncounterId};
use crate::engine::core::{tick_engine, with_suppressed_engine_warnings};
use crate::runtime::action::Action;
use crate::runtime::combat::{CardZones, CombatMeta, TurnRuntime};
use crate::runtime::combat::{CombatRng, CombatState, EngineRuntime, EntityState};
use crate::runtime::rng;
use crate::state::core::{CombatStartRequest, EngineState};
use crate::state::map::node::RoomType;
use crate::state::run::RunState;
use crate::state::selection::{EngineDiagnostic, EngineDiagnosticClass, EngineDiagnosticSeverity};

thread_local! {
    static OBSERVE_ENTRY_BIRTHS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Request an output-only birth receipt while a committed room action creates
/// its combat. Ordinary simulation does not enable this observer.
pub fn with_combat_entry_birth_observation<T>(execute: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            OBSERVE_ENTRY_BIRTHS.with(|enabled| enabled.set(self.0));
        }
    }
    let _reset = Reset(OBSERVE_ENTRY_BIRTHS.with(|enabled| enabled.replace(true)));
    execute()
}

pub fn build_natural_combat_start(
    run_state: &mut RunState,
    encounter_id: EncounterId,
    room_type: RoomType,
) -> Result<(EngineState, CombatState), String> {
    let monsters = factory::build_encounter(
        encounter_id,
        &mut run_state.rng_pool.misc_rng,
        &mut run_state.rng_pool.monster_hp_rng,
        run_state.ascension_level,
    );
    build_combat_start_with_monsters(run_state, room_type, monsters)
}

pub fn build_requested_combat_start(
    run: &mut RunState,
    request: &mut CombatStartRequest,
) -> Result<(EngineState, CombatState), String> {
    if let Some(prepared) = &request.prepared_encounter {
        if prepared.encounter_id != request.encounter_id {
            return Err("prepared encounter does not match the combat request".to_string());
        }
    }
    match request.prepared_encounter.take() {
        Some(prepared) => {
            build_combat_start_with_monsters(run, request.room_type, prepared.monsters)
        }
        None => build_natural_combat_start(run, request.encounter_id, request.room_type),
    }
}

fn build_combat_start_with_monsters(
    run_state: &mut RunState,
    room_type: RoomType,
    monsters: Vec<crate::runtime::combat::MonsterEntity>,
) -> Result<(EngineState, CombatState), String> {
    let player = run_state.build_combat_player(0);

    let mut combat = CombatState {
        meta: CombatMeta {
            ascension_level: run_state.ascension_level,
            player_class: run_state.player_class.to_string(),
            is_boss_fight: room_type == RoomType::MonsterRoomBoss,
            is_elite_fight: room_type == RoomType::MonsterRoomElite,
            master_deck_snapshot: run_state.master_deck.clone().into(),
            master_deck_lineage_allocator: run_state.master_deck_lineage_allocator.clone(),
            meta_changes: Vec::new(),
        },
        turn: TurnRuntime::fresh_player_turn(3),
        zones: CardZones {
            draw_pile: run_state.master_deck.clone().into(),
            hand: Vec::new(),
            discard_pile: Vec::new().into(),
            exhaust_pile: Vec::new().into(),
            limbo: Vec::new(),
            queued_cards: VecDeque::new(),
            combat_lineage_allocator: Default::default(),
        },
        entities: EntityState {
            player,
            monsters,
            potions: run_state.potions.clone(),
            power_db: Default::default(),
        },
        engine: EngineRuntime::new(),
        rng: CombatRng::new(run_state.rng_pool.clone()),
        runtime: Default::default(),
    };

    if OBSERVE_ENTRY_BIRTHS.with(|enabled| enabled.get()) {
        combat.begin_card_birth_observation(
            crate::runtime::combat::CombatCardBirthCause::CombatEntryResolution,
        );
    }
    roll_initial_monster_plans(&mut combat);

    combat.reset_turn_energy_from_player();
    rng::shuffle_with_random_long(
        combat.zones.draw_pile.as_mut_slice(),
        &mut combat.rng.shuffle_rng,
    );
    combat.apply_java_initialize_deck_order_after_shuffle();
    // AbstractPlayer.preBattlePrep initializes the deck before the room rolls
    // its emerald buff. Preserve that cross-stream RNG order as well as values.
    prepare_room_pre_battle_actions(run_state, room_type, &mut combat);
    combat.queue_action_back(Action::PreBattleTrigger);

    let mut engine_state = EngineState::CombatProcessing;
    let alive = with_suppressed_engine_warnings(|| {
        drain_to_player_boundary(&mut engine_state, &mut combat)
    });
    if !alive && !matches!(engine_state, EngineState::RewardScreen(_)) {
        return Err(format!(
            "combat initialization reached non-victory terminal state before player input: {engine_state:?}"
        ));
    }
    if alive
        && !matches!(
            engine_state,
            EngineState::CombatPlayerTurn | EngineState::PendingChoice(_)
        )
    {
        return Err(format!(
            "combat initialization did not reach stable player turn, got {engine_state:?}"
        ));
    }

    Ok((engine_state, combat))
}

fn prepare_room_pre_battle_actions(
    run_state: &mut RunState,
    room_type: RoomType,
    combat: &mut CombatState,
) {
    let is_burning_elite = room_type == RoomType::MonsterRoomElite
        && run_state.is_final_act_available
        && run_state
            .map
            .get_current_node()
            .is_some_and(|node| node.has_emerald_key);
    if !is_burning_elite {
        return;
    }

    // Java generates and assigns the map with AbstractDungeon.mapRng, marks
    // the emerald node with the same stream, then retains it until this roll.
    let roll = run_state.map_rng.random_range(0, 3);
    combat.runtime.room_pre_battle_actions =
        emerald_elite_actions_for_roll(roll, run_state.act_num, combat);
}

fn emerald_elite_actions_for_roll(roll: i32, act_num: u8, combat: &CombatState) -> Vec<Action> {
    use crate::content::powers::PowerId;

    let amount = match roll {
        0 => i32::from(act_num) + 1,
        2 => i32::from(act_num) * 2 + 2,
        3 => 1 + i32::from(act_num) * 2,
        _ => 0,
    };
    combat
        .entities
        .monsters
        .iter()
        .map(|monster| match roll {
            0 => Action::ApplyPower {
                source: monster.id,
                target: monster.id,
                power_id: PowerId::Strength,
                amount,
            },
            1 => Action::IncreaseMonsterMaxHpPercent {
                target: monster.id,
                percent: 25,
            },
            2 => Action::ApplyPower {
                source: monster.id,
                target: monster.id,
                power_id: PowerId::Metallicize,
                amount,
            },
            3 => Action::ApplyPower {
                source: monster.id,
                target: monster.id,
                power_id: PowerId::Regen,
                amount,
            },
            _ => unreachable!("mapRng.random(0, 3) returned {roll}"),
        })
        .collect()
}

pub fn encounter_id_from_input(raw: &str) -> Result<EncounterId, String> {
    let normalized = normalize_identifier(raw);
    match normalized.as_str() {
        "blueslaver" => Ok(EncounterId::BlueSlaver),
        "cultist" => Ok(EncounterId::Cultist),
        "jawworm" => Ok(EncounterId::JawWorm),
        "looter" => Ok(EncounterId::Looter),
        "gremlingang" => Ok(EncounterId::GremlinGang),
        "redslaver" => Ok(EncounterId::RedSlaver),
        "largeslime" => Ok(EncounterId::LargeSlime),
        "exordiumthugs" => Ok(EncounterId::ExordiumThugs),
        "exordiumwildlife" => Ok(EncounterId::ExordiumWildlife),
        "threelouse" | "3louse" => Ok(EncounterId::ThreeLouse),
        "twolouse" | "2louse" => Ok(EncounterId::TwoLouse),
        "twofungibeasts" | "2fungibeasts" => Ok(EncounterId::TwoFungiBeasts),
        "lotsofslimes" => Ok(EncounterId::LotsOfSlimes),
        "smallslimes" => Ok(EncounterId::SmallSlimes),
        "gremlinnob" | "nob" => Ok(EncounterId::GremlinNob),
        "lagavulin" => Ok(EncounterId::Lagavulin),
        "lagavulinevent" => Ok(EncounterId::LagavulinEvent),
        "threesentries" | "3sentries" | "sentries" => Ok(EncounterId::ThreeSentries),
        "themushroomlair" | "mushroomlair" => Ok(EncounterId::TheMushroomLair),
        "theguardian" | "guardian" => Ok(EncounterId::TheGuardian),
        "hexaghost" => Ok(EncounterId::Hexaghost),
        "slimeboss" => Ok(EncounterId::SlimeBoss),
        "twothieves" | "2thieves" => Ok(EncounterId::TwoThieves),
        "threebyrds" | "3byrds" => Ok(EncounterId::ThreeByrds),
        "chosenalone" | "chosen" => Ok(EncounterId::ChosenAlone),
        "shellparasite" => Ok(EncounterId::ShellParasite),
        "sphericguardian" => Ok(EncounterId::SphericGuardian),
        "chosenandbyrds" => Ok(EncounterId::ChosenAndByrds),
        "sentryandsphere" => Ok(EncounterId::SentryAndSphere),
        "snakeplant" => Ok(EncounterId::SnakePlant),
        "snecko" => Ok(EncounterId::Snecko),
        "centurionandhealer" => Ok(EncounterId::CenturionAndHealer),
        "cultistandchosen" => Ok(EncounterId::CultistAndChosen),
        "threecultists" | "3cultists" => Ok(EncounterId::ThreeCultists),
        "shelledparasiteandfungi" => Ok(EncounterId::ShelledParasiteAndFungi),
        "gremlinleader" => Ok(EncounterId::GremlinLeader),
        "slavers" => Ok(EncounterId::Slavers),
        "bookofstabbing" => Ok(EncounterId::BookOfStabbing),
        "automaton" | "bronzeautomaton" => Ok(EncounterId::Automaton),
        "thechamp" | "champ" => Ok(EncounterId::TheChamp),
        "collector" | "thecollector" => Ok(EncounterId::Collector),
        "maskedbandits" => Ok(EncounterId::MaskedBandits),
        "colosseumslavers" => Ok(EncounterId::ColosseumSlavers),
        "colosseumnobs" => Ok(EncounterId::ColosseumNobs),
        "threedarklings" | "3darklings" => Ok(EncounterId::ThreeDarklings),
        "orbwalker" => Ok(EncounterId::OrbWalker),
        "threeshapes" | "3shapes" => Ok(EncounterId::ThreeShapes),
        "spiregrowth" => Ok(EncounterId::SpireGrowth),
        "transient" | "thetransient" => Ok(EncounterId::Transient),
        "fourshapes" | "4shapes" => Ok(EncounterId::FourShapes),
        "themaw" | "maw" => Ok(EncounterId::TheMaw),
        "sphereandtwoshapes" | "sphereand2shapes" => Ok(EncounterId::SphereAndTwoShapes),
        "jawwormhorde" => Ok(EncounterId::JawWormHorde),
        "writhingmass" => Ok(EncounterId::WrithingMass),
        "gianthead" => Ok(EncounterId::GiantHead),
        "thenemesis" | "nemesis" => Ok(EncounterId::TheNemesis),
        "reptomancer" => Ok(EncounterId::Reptomancer),
        "awakenedone" | "theawakenedone" => Ok(EncounterId::AwakenedOne),
        "timeeater" => Ok(EncounterId::TimeEater),
        "donuanddeca" => Ok(EncounterId::DonuAndDeca),
        "mysterioussphere" => Ok(EncounterId::MysteriousSphere),
        "twoorbwalkers" | "2orbwalkers" => Ok(EncounterId::TwoOrbWalkers),
        "sneckoandmystics" => Ok(EncounterId::SneckoAndMystics),
        "shieldandspear" | "spearandshield" => Ok(EncounterId::ShieldAndSpear),
        "theheart" | "corruptheart" | "heart" => Ok(EncounterId::TheHeart),
        _ => Err(format!("unsupported encounter_id '{raw}'")),
    }
}

pub fn room_type_from_input(raw: &str) -> Result<RoomType, String> {
    let normalized = normalize_identifier(raw);
    match normalized.as_str() {
        "monsterroomboss" | "boss" => Ok(RoomType::MonsterRoomBoss),
        "monsterroomelite" | "elite" => Ok(RoomType::MonsterRoomElite),
        "monsterroom" | "monster" => Ok(RoomType::MonsterRoom),
        _ => Err(format!("unsupported room_type '{raw}'")),
    }
}

fn roll_initial_monster_plans(combat: &mut CombatState) {
    let monsters_clone = combat.entities.monsters.clone();
    let player_powers = crate::content::powers::store::powers_snapshot_for(combat, 0);
    let monster_ids = combat
        .entities
        .monsters
        .iter()
        .map(|monster| monster.id)
        .collect::<Vec<_>>();
    for monster_id in monster_ids {
        let entity_snapshot = combat
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == monster_id)
            .cloned()
            .expect("initial monster should exist while rolling intent");
        let num = combat.rng.ai_rng.random(99);
        let outcome = crate::content::monsters::roll_monster_turn_outcome(
            &mut combat.rng.ai_rng,
            &entity_snapshot,
            combat.meta.ascension_level,
            num,
            &monsters_clone,
            &player_powers,
        );
        for action in outcome.setup_actions {
            crate::engine::action_handlers::execute_action(action, combat);
        }
        let plan = outcome.plan;
        let monster = combat
            .entities
            .monsters
            .iter_mut()
            .find(|monster| monster.id == monster_id)
            .expect("rolled monster should still exist");
        monster.set_planned_move_id(plan.move_id);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        monster.move_history_mut().push_back(plan.move_id);
        combat
            .runtime
            .monster_protocol
            .entry(monster_id)
            .or_default()
            .observation = Default::default();
    }
}

fn drain_to_player_boundary(es: &mut EngineState, cs: &mut CombatState) -> bool {
    let mut iterations = 0;
    loop {
        match es {
            EngineState::CombatPlayerTurn => break,
            EngineState::CombatProcessing => {}
            EngineState::PendingChoice(_) => break,
            EngineState::GameOver(_) => return false,
            _ => break,
        }
        let alive = tick_engine(es, cs, None);
        if !alive {
            return false;
        }
        iterations += 1;
        if iterations > 1000 {
            cs.emit_diagnostic(EngineDiagnostic {
                severity: EngineDiagnosticSeverity::Warning,
                class: EngineDiagnosticClass::Suspicious,
                message: "tick loop exceeded 1000 iterations".to_string(),
            });
            break;
        }
    }
    true
}

fn normalize_identifier(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::build_natural_combat_start;
    use crate::content::cards::CardId;
    use crate::content::monsters::factory::EncounterId;
    use crate::content::powers::{store, PowerId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::OrbId;
    use crate::runtime::rng::StsRng;
    use crate::state::core::EngineState;
    use crate::state::map::node::{MapRoomNode, RoomType};
    use crate::state::map::state::MapState;
    use crate::state::run::RunState;

    #[test]
    fn observed_combat_entry_receipt_tracks_pure_water_without_changing_gameplay() {
        use crate::runtime::combat::{CombatCardBirthCause, CombatCardBirthZone};
        let mut run = RunState::new(2026101801, 20, true, "Watcher");
        let (_, ordinary) = build_natural_combat_start(
            &mut run.clone(),
            EncounterId::JawWorm,
            RoomType::MonsterRoom,
        )
        .unwrap();
        let (_, mut observed) = super::with_combat_entry_birth_observation(|| {
            build_natural_combat_start(&mut run, EncounterId::JawWorm, RoomType::MonsterRoom)
        })
        .unwrap();
        let receipt = observed.take_card_birth_observation().unwrap();
        assert!(receipt.complete);
        assert_eq!(receipt.cause, CombatCardBirthCause::CombatEntryResolution);
        assert_eq!(receipt.allocated_count, 1);
        assert_eq!(receipt.products.len(), 1);
        assert_eq!(receipt.products[0].card.id, CardId::Miracle);
        assert_eq!(
            receipt.products[0].first_zone,
            Some(CombatCardBirthZone::Hand)
        );
        assert_eq!(observed, ordinary);
        let (_, mut later) = build_natural_combat_start(
            &mut run.clone(),
            EncounterId::JawWorm,
            RoomType::MonsterRoom,
        )
        .unwrap();
        assert!(later.take_card_birth_observation().is_none());
    }

    fn set_current_burning_elite_node(run: &mut RunState) {
        let mut node = MapRoomNode::new(0, 0);
        node.class = Some(RoomType::MonsterRoomElite);
        node.has_emerald_key = true;
        run.map = MapState::new(vec![vec![node]]);
        run.map.current_x = 0;
        run.map.current_y = 0;
    }

    fn rng_whose_first_four_way_roll_is(expected: i32) -> StsRng {
        for seed in 1..10_000 {
            let mut rng = StsRng::new(seed);
            if rng.random_range(0, 3) == expected {
                return StsRng::new(seed);
            }
        }
        panic!("could not find deterministic map RNG fixture for roll {expected}");
    }

    #[test]
    fn burning_elite_roll_follows_opening_deck_shuffle_in_rng_ledger() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};

        let mut run = RunState::new(4, 20, true, "Ironclad");
        set_current_burning_elite_node(&mut run);
        run.map_rng.bind_trace_owner("run.map");
        let trace = begin_sts_rng_trace().unwrap();
        build_natural_combat_start(
            &mut run,
            EncounterId::GremlinLeader,
            RoomType::MonsterRoomElite,
        )
        .unwrap();
        let events = trace.finish().unwrap();
        let shuffle = events
            .iter()
            .position(|e| e.owner == Some("floor.shuffle"))
            .unwrap();
        let map = events
            .iter()
            .position(|e| e.owner == Some("run.map"))
            .unwrap();
        assert!(
            shuffle < map,
            "Java initializes the draw pile before rolling the room buff"
        );
        assert!(matches!(
            events[shuffle].call,
            StsRngCall::RandomLongBits { .. }
        ));
        assert!(matches!(
            events[map].call,
            StsRngCall::RandomIntRange {
                start: 0,
                end: 3,
                ..
            }
        ));
        assert_eq!(
            events.iter().filter(|e| e.owner == Some("run.map")).count(),
            1
        );
    }

    #[test]
    fn natural_combat_start_applies_ring_of_the_serpent_opening_hand_size() {
        let mut run = RunState::new(1, 0, false, "Silent");
        run.relics = vec![RelicState::new(RelicId::RingOfTheSerpent)];

        let (_engine_state, combat) =
            build_natural_combat_start(&mut run, EncounterId::JawWorm, RoomType::MonsterRoom)
                .expect("combat should initialize");

        assert_eq!(
            combat.zones.hand.len(),
            6,
            "Java Ring of the Serpent increments masterHandSize, so initial draw uses 6"
        );
    }

    #[test]
    fn natural_defect_combat_start_has_java_orb_slots_before_cracked_core() {
        let mut run = RunState::new(1, 0, false, "Defect");

        let (_engine_state, combat) =
            build_natural_combat_start(&mut run, EncounterId::JawWorm, RoomType::MonsterRoom)
                .expect("combat should initialize");

        assert_eq!(combat.entities.player.max_orbs, 3);
        assert_eq!(combat.entities.player.orbs.len(), 3);
        assert_eq!(
            combat.entities.player.orbs[0].id,
            OrbId::Lightning,
            "Java Defect starts combat with 3 master orb slots before Cracked Core channels Lightning"
        );
        assert!(combat
            .entities
            .player
            .orbs
            .iter()
            .skip(1)
            .all(|orb| orb.id == OrbId::Empty));
    }

    #[test]
    fn natural_non_defect_prismatic_shard_combat_start_has_one_empty_orb_slot() {
        let mut run = RunState::new(1, 0, false, "Silent");
        run.relics.push(RelicState::new(RelicId::PrismaticShard));

        let (_engine_state, combat) =
            build_natural_combat_start(&mut run, EncounterId::JawWorm, RoomType::MonsterRoom)
                .expect("combat should initialize");

        assert_eq!(
            combat.entities.player.max_orbs, 1,
            "Java PrismaticShard.onEquip grants one master orb slot to non-Defect classes"
        );
        assert_eq!(combat.entities.player.orbs.len(), 1);
        assert_eq!(combat.entities.player.orbs[0].id, OrbId::Empty);
    }

    #[test]
    fn natural_combat_start_keeps_combat_generation_in_an_independent_origin() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        assert!(run.add_card_to_deck(CardId::Armaments));
        let armaments_lineage = run
            .master_deck
            .iter()
            .find(|card| card.id == CardId::Armaments)
            .expect("added Armaments should be in the master deck")
            .lineage;

        let (_engine_state, mut combat) =
            build_natural_combat_start(&mut run, EncounterId::JawWorm, RoomType::MonsterRoom)
                .expect("combat should initialize");

        let generated = combat.allocate_combat_generated_lineage();
        assert_eq!(
            generated,
            crate::runtime::combat::CardLineageId::combat_generated(1)
        );
        assert_ne!(generated, armaments_lineage);
    }

    #[test]
    fn natural_combat_start_generated_cards_do_not_alias_master_deck_lineages() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        assert!(run.add_card_to_deck(CardId::Armaments));
        let armaments_lineage = run
            .master_deck
            .iter()
            .find(|card| card.id == CardId::Armaments)
            .expect("added Armaments should be in the master deck")
            .lineage;

        let (_engine_state, mut combat) =
            build_natural_combat_start(&mut run, EncounterId::JawWorm, RoomType::MonsterRoom)
                .expect("combat should initialize");

        crate::engine::action_handlers::cards::handle_make_temp_card_in_discard(
            CardId::Slimed,
            1,
            false,
            &mut combat,
        );

        let slimed_lineage = combat
            .zones
            .discard_pile
            .iter()
            .find(|card| card.id == CardId::Slimed)
            .map(|card| card.lineage);

        assert_eq!(
            slimed_lineage,
            Some(crate::runtime::combat::CardLineageId::combat_generated(1))
        );
        assert_ne!(slimed_lineage, Some(armaments_lineage));
        assert_eq!(
            combat
                .zones
                .draw_pile
                .iter()
                .chain(combat.zones.hand.iter())
                .chain(combat.zones.discard_pile.iter())
                .filter(|card| card.lineage == armaments_lineage)
                .count(),
            1
        );
    }

    #[test]
    fn natural_combat_start_preserves_victory_before_first_player_input() {
        let mut run = RunState::new(4, 20, false, "Ironclad");
        let mut lament = RelicState::new(RelicId::NeowsLament);
        lament.counter = 2;
        run.relics = vec![
            RelicState::new(RelicId::BurningBlood),
            lament,
            RelicState::new(RelicId::MercuryHourglass),
        ];

        let (engine_state, combat) =
            build_natural_combat_start(&mut run, EncounterId::Cultist, RoomType::MonsterRoom)
                .expect("a pre-input relic victory is a legal natural combat outcome");

        assert!(matches!(engine_state, EngineState::RewardScreen(_)));
        assert!(combat
            .entities
            .monsters
            .iter()
            .all(|monster| monster.current_hp <= 0));
    }

    #[test]
    fn burning_elite_applies_each_java_map_rng_buff_to_every_monster() {
        let mut baseline_run = RunState::new(77, 20, false, "Ironclad");
        baseline_run.act_num = 2;
        set_current_burning_elite_node(&mut baseline_run);
        let (_, baseline) = build_natural_combat_start(
            &mut baseline_run,
            EncounterId::GremlinNob,
            RoomType::MonsterRoomElite,
        )
        .expect("baseline elite should initialize");
        let baseline_monster = &baseline.entities.monsters[0];

        for roll in 0..=3 {
            let mut run = RunState::new(77, 20, true, "Ironclad");
            run.act_num = 2;
            set_current_burning_elite_node(&mut run);
            run.map_rng = rng_whose_first_four_way_roll_is(roll);

            let (_, combat) = build_natural_combat_start(
                &mut run,
                EncounterId::GremlinNob,
                RoomType::MonsterRoomElite,
            )
            .expect("burning elite should initialize");
            let monster = &combat.entities.monsters[0];
            assert_eq!(run.map_rng.counter, 1);

            match roll {
                0 => assert_eq!(
                    store::power_amount(&combat, monster.id, PowerId::Strength),
                    3
                ),
                1 => {
                    let increase = (baseline_monster.max_hp as f32 * 0.25).round() as i32;
                    assert_eq!(monster.max_hp, baseline_monster.max_hp + increase);
                    assert_eq!(monster.current_hp, baseline_monster.current_hp + increase);
                }
                2 => assert_eq!(
                    store::power_amount(&combat, monster.id, PowerId::Metallicize),
                    6
                ),
                3 => assert_eq!(store::power_amount(&combat, monster.id, PowerId::Regen), 5),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn seed_four_act_two_burning_elite_rolls_strength_three_like_java() {
        let mut run = RunState::new(4, 20, true, "Ironclad");
        run.advance_act();
        let (y, x) = run
            .map
            .graph
            .iter()
            .enumerate()
            .flat_map(|(y, row)| row.iter().enumerate().map(move |(x, node)| (y, x, node)))
            .find_map(|(y, x, node)| node.has_emerald_key.then_some((y, x)))
            .expect("Act 2 map should contain an emerald elite");
        run.map.current_y = y as i32;
        run.map.current_x = x as i32;

        let mut expected_roll = run.map_rng.clone();
        assert_eq!(expected_roll.random_range(0, 3), 0);
        let (_, combat) = build_natural_combat_start(
            &mut run,
            EncounterId::GremlinLeader,
            RoomType::MonsterRoomElite,
        )
        .expect("seed 4 burning Gremlin Leader should initialize");

        assert_eq!(combat.entities.monsters.len(), 3);
        for monster in &combat.entities.monsters {
            assert_eq!(
                store::power_amount(&combat, monster.id, PowerId::Strength),
                3
            );
        }
    }
}
