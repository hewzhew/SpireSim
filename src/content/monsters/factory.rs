use crate::runtime::combat::{MonsterEntity, MonsterMoveState};
use crate::runtime::rng::StsRng;
crate::stable_gameplay_id! {
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncounterId {
    // === Act 1: Exordium ===
    BlueSlaver => ("BlueSlaver", 0),
    Cultist => ("Cultist", 1),
    JawWorm => ("JawWorm", 2),
    Looter => ("Looter", 3),
    GremlinGang => ("GremlinGang", 4),
    RedSlaver => ("RedSlaver", 5),
    LargeSlime => ("LargeSlime", 6),
    ExordiumThugs => ("ExordiumThugs", 7),
    ExordiumWildlife => ("ExordiumWildlife", 8),
    ThreeLouse => ("ThreeLouse", 9),
    TwoLouse => ("TwoLouse", 10),
    TwoFungiBeasts => ("TwoFungiBeasts", 11),
    LotsOfSlimes => ("LotsOfSlimes", 12),
    SmallSlimes => ("SmallSlimes", 13),
    GremlinNob => ("GremlinNob", 14),
    Lagavulin => ("Lagavulin", 15),
    ThreeSentries => ("ThreeSentries", 16),
    LagavulinEvent => ("LagavulinEvent", 17),
    TheMushroomLair => ("TheMushroomLair", 18),
    TheGuardian => ("TheGuardian", 19),
    Hexaghost => ("Hexaghost", 20),
    SlimeBoss => ("SlimeBoss", 21),

    // === Act 2: The City ===
    // Weak
    TwoThieves => ("TwoThieves", 22),      // "2 Thieves" — Looter + Mugger
    ThreeByrds => ("ThreeByrds", 23),      // "3 Byrds"
    ChosenAlone => ("ChosenAlone", 24),     // "Chosen"
    ShellParasite => ("ShellParasite", 25),   // "Shell Parasite"
    SphericGuardian => ("SphericGuardian", 26), // "Spheric Guardian"
    // Strong
    ChosenAndByrds => ("ChosenAndByrds", 27),          // "Chosen and Byrds" — Byrd + Chosen
    SentryAndSphere => ("SentryAndSphere", 28),         // "Sentry and Sphere" — Sentry + SphericGuardian
    SnakePlant => ("SnakePlant", 29),              // "Snake Plant"
    Snecko => ("Snecko", 30),                  // "Snecko"
    CenturionAndHealer => ("CenturionAndHealer", 31),      // "Centurion and Healer"
    CultistAndChosen => ("CultistAndChosen", 32),        // "Cultist and Chosen"
    ThreeCultists => ("ThreeCultists", 33),           // "3 Cultists"
    ShelledParasiteAndFungi => ("ShelledParasiteAndFungi", 34), // "Shelled Parasite and Fungi"
    // Elite
    GremlinLeader => ("GremlinLeader", 35),  // "Gremlin Leader" — 2 gremlins + leader
    Slavers => ("Slavers", 36),        // "Slavers" — Blue + Taskmaster + Red
    BookOfStabbing => ("BookOfStabbing", 37), // "Book of Stabbing"
    // Boss
    Automaton => ("Automaton", 38), // "Automaton" — BronzeAutomaton
    TheChamp => ("TheChamp", 39),  // "The Champ"
    Collector => ("Collector", 40), // "Collector" — TheCollector
    // Event
    MaskedBandits => ("MaskedBandits", 41),    // "Masked Bandits"
    ColosseumSlavers => ("ColosseumSlavers", 42), // "Colosseum Slavers"
    ColosseumNobs => ("ColosseumNobs", 43),    // "Colosseum Nobs"

    // === Act 3: The Beyond ===
    // Weak
    ThreeDarklings => ("ThreeDarklings", 44), // "3 Darklings"
    OrbWalker => ("OrbWalker", 45),      // "Orb Walker"
    ThreeShapes => ("ThreeShapes", 46),    // "3 Shapes" — random draw from Spiker/Repulsor/Exploder
    // Strong
    SpireGrowth => ("SpireGrowth", 47),        // "Spire Growth"
    Transient => ("Transient", 48),          // "Transient"
    FourShapes => ("FourShapes", 49),         // "4 Shapes"
    TheMaw => ("TheMaw", 50),             // "The Maw"
    SphereAndTwoShapes => ("SphereAndTwoShapes", 51), // "Sphere and 2 Shapes"
    JawWormHorde => ("JawWormHorde", 52),       // "Jaw Worm Horde" — 3 JawWorms (harder)
    WrithingMass => ("WrithingMass", 53),       // "Writhing Mass"
    // Elite
    GiantHead => ("GiantHead", 54),   // "Giant Head"
    TheNemesis => ("TheNemesis", 55),  // "Nemesis"
    Reptomancer => ("Reptomancer", 56), // "Reptomancer" — Reptomancer + SnakeDaggers
    // Boss
    AwakenedOne => ("AwakenedOne", 57), // "Awakened One" — 2 Cultists + AwakenedOne
    TimeEater => ("TimeEater", 58),   // "Time Eater"
    DonuAndDeca => ("DonuAndDeca", 59), // "Donu and Deca"
    // Event
    MysteriousSphere => ("MysteriousSphere", 60), // "Mysterious Sphere" — 2 Shapes + OrbWalker
    TwoOrbWalkers => ("TwoOrbWalkers", 61),    // "2 Orb Walkers"
    SneckoAndMystics => ("SneckoAndMystics", 62), // "Snecko and Mystics" — Healer + Snecko + Healer

    // === Act 4 ===
    ShieldAndSpear => ("ShieldAndSpear", 63), // "Shield and Spear"
    TheHeart => ("TheHeart", 64),       // "The Heart" — CorruptHeart
}
}

impl crate::content::identity::GameplayIdentitySurface for EncounterId {
    fn display_name_v1(self) -> Option<&'static str> {
        use super::EnemyId;
        // Boss-map readings use the in-game names of the known encounter.
        // Stable encounter IDs continue to own serialization and equality.
        let boss = match self {
            Self::GremlinNob => EnemyId::GremlinNob,
            Self::Lagavulin => EnemyId::Lagavulin,
            Self::ThreeSentries => return Some("3 Sentries"),
            Self::TheGuardian => EnemyId::TheGuardian,
            Self::Hexaghost => EnemyId::Hexaghost,
            Self::SlimeBoss => EnemyId::SlimeBoss,
            Self::Automaton => EnemyId::BronzeAutomaton,
            Self::TheChamp => EnemyId::Champ,
            Self::Collector => EnemyId::TheCollector,
            Self::AwakenedOne => EnemyId::AwakenedOne,
            Self::TimeEater => EnemyId::TimeEater,
            Self::DonuAndDeca => return Some("Donu and Deca"),
            Self::TheHeart => EnemyId::CorruptHeart,
            _ => return None,
        };
        Some(boss.get_name())
    }

    fn input_aliases_v1(self) -> Vec<&'static str> {
        match self {
            Self::ThreeSentries => vec!["Sentries"],
            _ => vec![],
        }
    }
}

use crate::content::monsters::EnemyId;

const SLAVERS: [EnemyId; 3] = [EnemyId::SlaverBlue, EnemyId::Taskmaster, EnemyId::SlaverRed];
const BOOK_OF_STABBING: [EnemyId; 1] = [EnemyId::BookOfStabbing];
const THE_HEART: [EnemyId; 1] = [EnemyId::CorruptHeart];
const GREMLIN_NOB: [EnemyId; 1] = [EnemyId::GremlinNob];
const LAGAVULIN: [EnemyId; 1] = [EnemyId::Lagavulin];
const THREE_SENTRIES: [EnemyId; 3] = [EnemyId::Sentry; 3];
const THE_GUARDIAN: [EnemyId; 1] = [EnemyId::TheGuardian];
const HEXAGHOST: [EnemyId; 1] = [EnemyId::Hexaghost];
const SLIME_BOSS: [EnemyId; 1] = [EnemyId::SlimeBoss];
const AUTOMATON: [EnemyId; 1] = [EnemyId::BronzeAutomaton];
const THE_CHAMP: [EnemyId; 1] = [EnemyId::Champ];
const COLLECTOR: [EnemyId; 1] = [EnemyId::TheCollector];
const GIANT_HEAD: [EnemyId; 1] = [EnemyId::GiantHead];
const THE_NEMESIS: [EnemyId; 1] = [EnemyId::Nemesis];
const REPTOMANCER: [EnemyId; 3] = [
    EnemyId::SnakeDagger,
    EnemyId::Reptomancer,
    EnemyId::SnakeDagger,
];
const AWAKENED_ONE: [EnemyId; 3] = [EnemyId::Cultist, EnemyId::Cultist, EnemyId::AwakenedOne];
const TIME_EATER: [EnemyId; 1] = [EnemyId::TimeEater];
const DONU_AND_DECA: [EnemyId; 2] = [EnemyId::Deca, EnemyId::Donu];
const SHIELD_AND_SPEAR: [EnemyId; 2] = [EnemyId::SpireShield, EnemyId::SpireSpear];
const MASKED_BANDITS: [EnemyId; 3] = [
    EnemyId::BanditPointy,
    EnemyId::BanditLeader,
    EnemyId::BanditBear,
];
const COLOSSEUM_SLAVERS: [EnemyId; 2] = [EnemyId::SlaverBlue, EnemyId::SlaverRed];
const COLOSSEUM_NOBS: [EnemyId; 2] = [EnemyId::Taskmaster, EnemyId::GremlinNob];

/// Public starting composition shared with encounter construction. Repeated random pool
/// entries express selection weights; construction and inspection share them.
pub struct EncounterLineup {
    pub fixed: &'static [EnemyId],
    pub random_slots: usize,
    pub random_pool: &'static [EnemyId],
}

pub fn encounter_lineup(encounter: EncounterId) -> Option<EncounterLineup> {
    let (fixed, random_slots, random_pool) = match encounter {
        EncounterId::GremlinNob => (GREMLIN_NOB.as_slice(), 0, &[][..]),
        EncounterId::Lagavulin => (LAGAVULIN.as_slice(), 0, &[][..]),
        EncounterId::ThreeSentries => (THREE_SENTRIES.as_slice(), 0, &[][..]),
        EncounterId::TheGuardian => (THE_GUARDIAN.as_slice(), 0, &[][..]),
        EncounterId::Hexaghost => (HEXAGHOST.as_slice(), 0, &[][..]),
        EncounterId::SlimeBoss => (SLIME_BOSS.as_slice(), 0, &[][..]),
        EncounterId::Automaton => (AUTOMATON.as_slice(), 0, &[][..]),
        EncounterId::TheChamp => (THE_CHAMP.as_slice(), 0, &[][..]),
        EncounterId::Collector => (COLLECTOR.as_slice(), 0, &[][..]),
        EncounterId::GiantHead => (GIANT_HEAD.as_slice(), 0, &[][..]),
        EncounterId::TheNemesis => (THE_NEMESIS.as_slice(), 0, &[][..]),
        EncounterId::Reptomancer => (REPTOMANCER.as_slice(), 0, &[][..]),
        EncounterId::AwakenedOne => (AWAKENED_ONE.as_slice(), 0, &[][..]),
        EncounterId::TimeEater => (TIME_EATER.as_slice(), 0, &[][..]),
        EncounterId::DonuAndDeca => (DONU_AND_DECA.as_slice(), 0, &[][..]),
        EncounterId::ShieldAndSpear => (SHIELD_AND_SPEAR.as_slice(), 0, &[][..]),
        EncounterId::Slavers => (SLAVERS.as_slice(), 0, &[][..]),
        EncounterId::BookOfStabbing => (BOOK_OF_STABBING.as_slice(), 0, &[][..]),
        EncounterId::MaskedBandits => (MASKED_BANDITS.as_slice(), 0, &[][..]),
        EncounterId::ColosseumSlavers => (COLOSSEUM_SLAVERS.as_slice(), 0, &[][..]),
        EncounterId::ColosseumNobs => (COLOSSEUM_NOBS.as_slice(), 0, &[][..]),
        EncounterId::TheHeart => (THE_HEART.as_slice(), 0, &[][..]),
        EncounterId::GremlinLeader => (
            &[EnemyId::GremlinLeader][..],
            2,
            super::city::gremlin_leader::SUMMON_POOL.as_slice(),
        ),
        _ => return None,
    };
    Some(EncounterLineup {
        fixed,
        random_slots,
        random_pool,
    })
}

/// Builds a complete encounter with seeded HP and precise composition parity with Java.
pub fn build_encounter(
    encounter: EncounterId,
    misc_rng: &mut StsRng,
    monster_hp_rng: &mut StsRng,
    ascension_level: u8,
) -> Vec<MonsterEntity> {
    let mut monsters = Vec::new();
    let mut slot_counter = 0;

    let spawn_monster = |enemy_id: EnemyId, hp_rng: &mut StsRng, slot: u8| -> MonsterEntity {
        let (min, max) = crate::content::monsters::get_hp_range(enemy_id, ascension_level);
        let current_hp = match enemy_id {
            // These constructors pass HP directly to super without setHp.
            // Do not generalize to min == max: setHp(hp) still consumes RNG.
            EnemyId::SphericGuardian => 20,
            EnemyId::Maw => 300,
            EnemyId::Transient => 999,
            EnemyId::Taskmaster => {
                // Taskmaster samples super's HP before its ascension setHp.
                let _ = hp_rng.random_range(54, 60);
                hp_rng.random_range(min, max)
            }
            EnemyId::OrbWalker => {
                // OrbWalker passes a random 90..96 HP to super, then setHp
                // consumes a second roll using the ascension-specific range.
                let _ = hp_rng.random_range(90, 96);
                hp_rng.random_range(min, max)
            }
            EnemyId::Reptomancer => {
                // Reptomancer samples super's HP before its ascension setHp.
                let _ = hp_rng.random_range(180, 190);
                hp_rng.random_range(min, max)
            }
            _ => hp_rng.random_range(min, max),
        };
        let id = (slot + 1) as usize; // Naive unique engine IDs for this batch
        let louse_bite_damage = match enemy_id {
            EnemyId::LouseNormal | EnemyId::LouseDefensive => {
                let (min, max) = super::exordium::louse_bite_damage_range(ascension_level);
                Some(hp_rng.random_range(min, max))
            }
            _ => None,
        };

        let mut monster = MonsterEntity {
            id,
            enemy_id,
            current_hp,
            max_hp: current_hp,
            block: 0,
            slot,
            is_dying: false,
            is_escaped: false,
            half_dead: false,
            move_state: MonsterMoveState::default(),
            logical_position: slot as i32,
            runtime: Default::default(),
        };
        monster.louse.bite_damage = louse_bite_damage;

        if enemy_id == EnemyId::Byrd {
            monster.byrd.first_move = true;
            monster.byrd.is_flying = true;
            monster.byrd.protocol_seeded = true;
        }
        if enemy_id == EnemyId::Chosen {
            monster.chosen.first_turn = true;
            monster.chosen.used_hex = false;
            monster.chosen.protocol_seeded = true;
        }
        if enemy_id == EnemyId::Snecko {
            monster.snecko.first_turn = true;
            monster.snecko.protocol_seeded = true;
        }
        if enemy_id == EnemyId::ShelledParasite {
            monster.shelled_parasite.first_move = true;
            monster.shelled_parasite.protocol_seeded = true;
        }
        if enemy_id == EnemyId::SphericGuardian {
            crate::content::monsters::city::spheric_guardian::initialize_runtime_state(
                &mut monster,
            );
        }
        if enemy_id == EnemyId::JawWorm {
            crate::content::monsters::exordium::jaw_worm::initialize_runtime_state(
                &mut monster,
                false,
            );
        }
        if enemy_id == EnemyId::BronzeAutomaton {
            monster.bronze_automaton.protocol_seeded = true;
            monster.bronze_automaton.first_turn = true;
            monster.bronze_automaton.num_turns = 0;
        }
        if enemy_id == EnemyId::BronzeOrb {
            monster.bronze_orb.protocol_seeded = true;
            monster.bronze_orb.used_stasis = false;
        }
        if enemy_id == EnemyId::BookOfStabbing {
            monster.book_of_stabbing.protocol_seeded = true;
            monster.book_of_stabbing.stab_count = 1;
        }
        if enemy_id == EnemyId::TheCollector {
            monster.collector.protocol_seeded = true;
            monster.collector.initial_spawn = true;
            monster.collector.ult_used = false;
            monster.collector.turns_taken = 0;
        }
        if enemy_id == EnemyId::Champ {
            monster.champ.protocol_seeded = true;
            monster.champ.first_turn = true;
            monster.champ.num_turns = 0;
            monster.champ.forge_times = 0;
            monster.champ.threshold_reached = false;
        }
        if enemy_id == EnemyId::AwakenedOne {
            monster.awakened_one.protocol_seeded = true;
            monster.awakened_one.form1 = true;
            monster.awakened_one.first_turn = true;
        }
        if enemy_id == EnemyId::CorruptHeart {
            crate::content::monsters::ending::corrupt_heart::initialize_runtime_state(
                &mut monster,
                ascension_level,
            );
        }
        if enemy_id == EnemyId::WrithingMass {
            monster.writhing_mass.protocol_seeded = true;
            monster.writhing_mass.first_move = true;
            monster.writhing_mass.used_mega_debuff = false;
        }
        if enemy_id == EnemyId::Spiker {
            monster.spiker.protocol_seeded = true;
            monster.spiker.thorns_count = 0;
        }
        if enemy_id == EnemyId::SpireShield {
            monster.spire_shield.protocol_seeded = true;
            monster.spire_shield.move_count = 0;
            monster.logical_position = -1;
        }
        if enemy_id == EnemyId::SpireSpear {
            crate::content::monsters::ending::spire_spear::initialize_runtime_state(
                &mut monster,
                ascension_level,
            );
            monster.logical_position = 1;
        }
        if enemy_id == EnemyId::SlaverRed {
            monster.slaver_red.protocol_seeded = true;
            monster.slaver_red.first_turn = true;
            monster.slaver_red.used_entangle = false;
        }
        if enemy_id == EnemyId::GremlinLeader {
            monster.gremlin_leader.protocol_seeded = true;
            monster.gremlin_leader.gremlin_slots = [None, None, None];
        }
        if enemy_id == EnemyId::GremlinNob {
            monster.gremlin_nob.protocol_seeded = true;
            monster.gremlin_nob.used_bellow = false;
        }
        if enemy_id == EnemyId::GremlinWizard {
            crate::content::monsters::exordium::gremlin_wizard::initialize_runtime_state(
                &mut monster,
            );
        }
        if enemy_id == EnemyId::Cultist {
            monster.cultist.protocol_seeded = true;
            monster.cultist.first_move = true;
        }
        if enemy_id == EnemyId::Sentry {
            monster.sentry.protocol_seeded = true;
            monster.sentry.first_move = true;
        }
        if enemy_id == EnemyId::SlimeBoss {
            crate::content::monsters::exordium::slime_boss::initialize_runtime_state(&mut monster);
        }
        if matches!(enemy_id, EnemyId::AcidSlimeL | EnemyId::SpikeSlimeL) {
            crate::content::monsters::exordium::initialize_large_slime_runtime_state(&mut monster);
        }
        if matches!(enemy_id, EnemyId::Looter | EnemyId::Mugger) {
            monster.thief.protocol_seeded = true;
            monster.thief.slash_count = 0;
            monster.thief.stolen_gold = 0;
        }

        if enemy_id == EnemyId::Darkling {
            crate::content::monsters::beyond::darkling::initialize_runtime_state(
                &mut monster,
                hp_rng,
                ascension_level,
            );
        }
        if enemy_id == EnemyId::Reptomancer {
            crate::content::monsters::beyond::reptomancer::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::Nemesis {
            crate::content::monsters::beyond::nemesis::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::GiantHead {
            crate::content::monsters::beyond::giant_head::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::TimeEater {
            crate::content::monsters::beyond::time_eater::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::Donu {
            crate::content::monsters::beyond::donu::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::Deca {
            crate::content::monsters::beyond::deca::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::Transient {
            crate::content::monsters::beyond::transient::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::Exploder {
            crate::content::monsters::beyond::exploder::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::Maw {
            crate::content::monsters::beyond::maw::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::SnakeDagger {
            crate::content::monsters::beyond::snake_dagger::initialize_runtime_state(&mut monster);
        }
        if enemy_id == EnemyId::TheGuardian {
            crate::content::monsters::exordium::the_guardian::initialize_runtime_state(
                &mut monster,
                ascension_level,
            );
        }

        monster
    };

    match encounter {
        EncounterId::BlueSlaver => {
            monsters.push(spawn_monster(
                EnemyId::SlaverBlue,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::Cultist => {
            monsters.push(spawn_monster(
                EnemyId::Cultist,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::JawWorm => {
            monsters.push(spawn_monster(
                EnemyId::JawWorm,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::Looter => {
            monsters.push(spawn_monster(EnemyId::Looter, monster_hp_rng, slot_counter));
        }
        EncounterId::GremlinGang => {
            // Java: spawnGremlins() — draw-without-replace from pool of 8, pick 4
            let mut pool = vec![
                EnemyId::GremlinWarrior,
                EnemyId::GremlinWarrior,
                EnemyId::GremlinThief,
                EnemyId::GremlinThief,
                EnemyId::GremlinFat,
                EnemyId::GremlinFat,
                EnemyId::GremlinTsundere,
                EnemyId::GremlinWizard,
            ];
            for _ in 0..4 {
                let index = misc_rng.random((pool.len() - 1) as i32) as usize;
                let picked = pool.remove(index);
                monsters.push(spawn_monster(picked, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::RedSlaver => {
            monsters.push(spawn_monster(
                EnemyId::SlaverRed,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::LargeSlime => {
            let is_acid = misc_rng.random_boolean();
            let slime_id = if is_acid {
                EnemyId::AcidSlimeL
            } else {
                EnemyId::SpikeSlimeL
            };
            monsters.push(spawn_monster(slime_id, monster_hp_rng, slot_counter));
        }
        EncounterId::ExordiumThugs => {
            // Java: bottomHumanoid() — weakWildlife + strongHumanoid

            // Java constructs all bottomGetWeakWildlife candidates before
            // picking one, so discarded candidates still consume constructor
            // monsterHpRng rolls.
            let get_louse = |rng: &mut StsRng| -> EnemyId {
                if rng.random_boolean() {
                    EnemyId::LouseNormal
                } else {
                    EnemyId::LouseDefensive
                }
            };
            let mut weak_candidates = vec![
                spawn_monster(get_louse(misc_rng), monster_hp_rng, slot_counter),
                spawn_monster(EnemyId::SpikeSlimeM, monster_hp_rng, slot_counter),
                spawn_monster(EnemyId::AcidSlimeM, monster_hp_rng, slot_counter),
            ];
            let weak_idx = misc_rng.random_range(0, 2) as usize;
            monsters.push(weak_candidates.remove(weak_idx));
            slot_counter += 1;

            // Java constructs Cultist, getSlaver(), and Looter before picking
            // one. Cultist's HP roll precedes getSlaver's miscRng color roll.
            let cultist = spawn_monster(EnemyId::Cultist, monster_hp_rng, slot_counter);
            let slaver = if misc_rng.random_boolean() {
                EnemyId::SlaverRed
            } else {
                EnemyId::SlaverBlue
            };
            let mut strong_candidates = vec![
                cultist,
                spawn_monster(slaver, monster_hp_rng, slot_counter),
                spawn_monster(EnemyId::Looter, monster_hp_rng, slot_counter),
            ];
            let strong_idx = misc_rng.random_range(0, 2) as usize;
            monsters.push(strong_candidates.remove(strong_idx));
        }
        EncounterId::ExordiumWildlife => {
            // Java: bottomWildlife() — numMonster=2: strongWildlife + weakWildlife

            // Java constructs both bottomGetStrongWildlife candidates before
            // selecting one.
            let mut strong_candidates = vec![
                spawn_monster(EnemyId::FungiBeast, monster_hp_rng, slot_counter),
                spawn_monster(EnemyId::JawWorm, monster_hp_rng, slot_counter),
            ];
            let strong_idx = misc_rng.random_range(0, 1) as usize;
            monsters.push(strong_candidates.remove(strong_idx));
            slot_counter += 1;

            // Same bottomGetWeakWildlife constructor-before-selection behavior
            // as Exordium Thugs.
            let louse = if misc_rng.random_boolean() {
                EnemyId::LouseNormal
            } else {
                EnemyId::LouseDefensive
            };
            let mut weak_candidates = vec![
                spawn_monster(louse, monster_hp_rng, slot_counter),
                spawn_monster(EnemyId::SpikeSlimeM, monster_hp_rng, slot_counter),
                spawn_monster(EnemyId::AcidSlimeM, monster_hp_rng, slot_counter),
            ];
            let weak_idx = misc_rng.random_range(0, 2) as usize;
            monsters.push(weak_candidates.remove(weak_idx));
        }
        EncounterId::ThreeLouse => {
            // Java: getLouse() × 3 — each independently random Normal/Defensive
            for _ in 0..3 {
                let louse = if misc_rng.random_boolean() {
                    EnemyId::LouseNormal
                } else {
                    EnemyId::LouseDefensive
                };
                monsters.push(spawn_monster(louse, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::TwoLouse => {
            // Java: getLouse() × 2
            for _ in 0..2 {
                let louse = if misc_rng.random_boolean() {
                    EnemyId::LouseNormal
                } else {
                    EnemyId::LouseDefensive
                };
                monsters.push(spawn_monster(louse, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::TwoFungiBeasts => {
            for _ in 0..2 {
                monsters.push(spawn_monster(
                    EnemyId::FungiBeast,
                    monster_hp_rng,
                    slot_counter,
                ));
                slot_counter += 1;
            }
        }
        EncounterId::LotsOfSlimes => {
            // Java: spawnManySmallSlimes() — draw-without-replace from pool of 5
            let mut pool = vec![
                EnemyId::SpikeSlimeS,
                EnemyId::SpikeSlimeS,
                EnemyId::SpikeSlimeS,
                EnemyId::AcidSlimeS,
                EnemyId::AcidSlimeS,
            ];
            for _ in 0..5 {
                let index = misc_rng.random((pool.len() - 1) as i32) as usize;
                let picked = pool.remove(index);
                monsters.push(spawn_monster(picked, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::SmallSlimes => {
            let is_spike = misc_rng.random_boolean();
            if is_spike {
                monsters.push(spawn_monster(
                    EnemyId::SpikeSlimeS,
                    monster_hp_rng,
                    slot_counter,
                ));
                slot_counter += 1;
                monsters.push(spawn_monster(
                    EnemyId::AcidSlimeM,
                    monster_hp_rng,
                    slot_counter,
                ));
            } else {
                monsters.push(spawn_monster(
                    EnemyId::AcidSlimeS,
                    monster_hp_rng,
                    slot_counter,
                ));
                slot_counter += 1;
                monsters.push(spawn_monster(
                    EnemyId::SpikeSlimeM,
                    monster_hp_rng,
                    slot_counter,
                ));
            }
        }
        EncounterId::GremlinNob => {
            monsters.push(spawn_monster(GREMLIN_NOB[0], monster_hp_rng, slot_counter));
        }
        EncounterId::Lagavulin | EncounterId::LagavulinEvent => {
            let mut lagavulin = spawn_monster(LAGAVULIN[0], monster_hp_rng, slot_counter);
            if matches!(encounter, EncounterId::LagavulinEvent) {
                lagavulin.lagavulin.is_out = true;
                lagavulin.lagavulin.is_out_triggered = true;
            }
            monsters.push(lagavulin);
        }
        EncounterId::ThreeSentries => {
            for enemy in THREE_SENTRIES {
                monsters.push(spawn_monster(enemy, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::TheMushroomLair => {
            for _ in 0..3 {
                monsters.push(spawn_monster(
                    EnemyId::FungiBeast,
                    monster_hp_rng,
                    slot_counter,
                ));
                slot_counter += 1;
            }
        }
        EncounterId::TheGuardian => {
            monsters.push(spawn_monster(THE_GUARDIAN[0], monster_hp_rng, slot_counter));
        }
        EncounterId::Hexaghost => {
            monsters.push(spawn_monster(HEXAGHOST[0], monster_hp_rng, slot_counter));
        }
        EncounterId::SlimeBoss => {
            monsters.push(spawn_monster(SLIME_BOSS[0], monster_hp_rng, slot_counter));
        }

        // ==========================================
        // Act 2: The City
        // ==========================================
        EncounterId::TwoThieves => {
            // Java: Looter + Mugger
            monsters.push(spawn_monster(EnemyId::Looter, monster_hp_rng, slot_counter));
            slot_counter += 1;
            monsters.push(spawn_monster(EnemyId::Mugger, monster_hp_rng, slot_counter));
        }
        EncounterId::ThreeByrds => {
            for _ in 0..3 {
                monsters.push(spawn_monster(EnemyId::Byrd, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::ChosenAlone => {
            monsters.push(spawn_monster(EnemyId::Chosen, monster_hp_rng, slot_counter));
        }
        EncounterId::ShellParasite => {
            monsters.push(spawn_monster(
                EnemyId::ShelledParasite,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::SphericGuardian => {
            monsters.push(spawn_monster(
                EnemyId::SphericGuardian,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::ChosenAndByrds => {
            // Java: Byrd + Chosen
            monsters.push(spawn_monster(EnemyId::Byrd, monster_hp_rng, slot_counter));
            slot_counter += 1;
            monsters.push(spawn_monster(EnemyId::Chosen, monster_hp_rng, slot_counter));
        }
        EncounterId::SentryAndSphere => {
            // Java: Sentry + SphericGuardian
            monsters.push(spawn_monster(EnemyId::Sentry, monster_hp_rng, slot_counter));
            slot_counter += 1;
            monsters.push(spawn_monster(
                EnemyId::SphericGuardian,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::SnakePlant => {
            monsters.push(spawn_monster(
                EnemyId::SnakePlant,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::Snecko => {
            monsters.push(spawn_monster(EnemyId::Snecko, monster_hp_rng, slot_counter));
        }
        EncounterId::CenturionAndHealer => {
            monsters.push(spawn_monster(
                EnemyId::Centurion,
                monster_hp_rng,
                slot_counter,
            ));
            slot_counter += 1;
            monsters.push(spawn_monster(EnemyId::Healer, monster_hp_rng, slot_counter));
        }
        EncounterId::CultistAndChosen => {
            // Java: Cultist(-230, 15, false) + Chosen(100, 25)
            monsters.push(spawn_monster(
                EnemyId::Cultist,
                monster_hp_rng,
                slot_counter,
            ));
            slot_counter += 1;
            monsters.push(spawn_monster(EnemyId::Chosen, monster_hp_rng, slot_counter));
        }
        EncounterId::ThreeCultists => {
            for _ in 0..3 {
                monsters.push(spawn_monster(
                    EnemyId::Cultist,
                    monster_hp_rng,
                    slot_counter,
                ));
                slot_counter += 1;
            }
        }
        EncounterId::ShelledParasiteAndFungi => {
            monsters.push(spawn_monster(
                EnemyId::ShelledParasite,
                monster_hp_rng,
                slot_counter,
            ));
            slot_counter += 1;
            monsters.push(spawn_monster(
                EnemyId::FungiBeast,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::GremlinLeader => {
            // Java: spawnGremlin() + spawnGremlin() + GremlinLeader
            // spawnGremlin = weighted random from the 8-entry pool used by MonsterHelper.
            let gremlin_pool = super::city::gremlin_leader::SUMMON_POOL;
            let g1 = gremlin_pool[misc_rng.random_range(0, 7) as usize];
            let mut first = spawn_monster(g1, monster_hp_rng, slot_counter);
            first.logical_position =
                crate::content::monsters::city::gremlin_leader::GremlinLeader::natural_minion_draw_x(0, g1);
            let first_id = first.id;
            monsters.push(first);
            slot_counter += 1;
            let g2 = gremlin_pool[misc_rng.random_range(0, 7) as usize];
            let mut second = spawn_monster(g2, monster_hp_rng, slot_counter);
            second.logical_position =
                crate::content::monsters::city::gremlin_leader::GremlinLeader::natural_minion_draw_x(1, g2);
            let second_id = second.id;
            monsters.push(second);
            slot_counter += 1;
            let mut leader = spawn_monster(EnemyId::GremlinLeader, monster_hp_rng, slot_counter);
            leader.logical_position =
                crate::content::monsters::city::gremlin_leader::GremlinLeader::LEADER_LOGICAL_POSITION;
            leader.gremlin_leader.protocol_seeded = true;
            leader.gremlin_leader.gremlin_slots = [Some(first_id), Some(second_id), None];
            monsters.push(leader);
        }
        EncounterId::Slavers => {
            // Java: SlaverBlue + Taskmaster + SlaverRed
            for enemy in SLAVERS {
                monsters.push(spawn_monster(enemy, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::BookOfStabbing => {
            monsters.push(spawn_monster(
                BOOK_OF_STABBING[0],
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::Automaton => {
            monsters.push(spawn_monster(AUTOMATON[0], monster_hp_rng, slot_counter));
        }
        EncounterId::TheChamp => {
            monsters.push(spawn_monster(THE_CHAMP[0], monster_hp_rng, slot_counter));
        }
        EncounterId::Collector => {
            monsters.push(spawn_monster(COLLECTOR[0], monster_hp_rng, slot_counter));
        }
        EncounterId::MaskedBandits => {
            // Java: BanditPointy + BanditLeader + BanditBear
            for enemy in MASKED_BANDITS {
                monsters.push(spawn_monster(enemy, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::ColosseumSlavers => {
            // Java: SlaverBlue + SlaverRed
            for enemy in COLOSSEUM_SLAVERS {
                monsters.push(spawn_monster(enemy, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::ColosseumNobs => {
            // Java: Taskmaster + GremlinNob
            for enemy in COLOSSEUM_NOBS {
                monsters.push(spawn_monster(enemy, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }

        // ==========================================
        // Act 3: The Beyond
        // ==========================================
        EncounterId::ThreeDarklings => {
            for _ in 0..3 {
                monsters.push(spawn_monster(
                    EnemyId::Darkling,
                    monster_hp_rng,
                    slot_counter,
                ));
                slot_counter += 1;
            }
        }
        EncounterId::OrbWalker => {
            monsters.push(spawn_monster(
                EnemyId::OrbWalker,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::ThreeShapes => {
            // Java: spawnShapes(true) — 3 random shapes, draw-without-replace
            let mut shape_pool = vec![
                EnemyId::Repulsor,
                EnemyId::Repulsor,
                EnemyId::Exploder,
                EnemyId::Exploder,
                EnemyId::Spiker,
                EnemyId::Spiker,
            ];
            for _ in 0..3 {
                let idx = misc_rng.random((shape_pool.len() - 1) as i32) as usize;
                let picked = shape_pool.remove(idx);
                monsters.push(spawn_monster(picked, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::SpireGrowth => {
            monsters.push(spawn_monster(
                EnemyId::SpireGrowth,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::Transient => {
            monsters.push(spawn_monster(
                EnemyId::Transient,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::FourShapes => {
            // Java: spawnShapes(false) — 4 random shapes, draw-without-replace
            let mut shape_pool = vec![
                EnemyId::Repulsor,
                EnemyId::Repulsor,
                EnemyId::Exploder,
                EnemyId::Exploder,
                EnemyId::Spiker,
                EnemyId::Spiker,
            ];
            for _ in 0..4 {
                let idx = misc_rng.random((shape_pool.len() - 1) as i32) as usize;
                let picked = shape_pool.remove(idx);
                monsters.push(spawn_monster(picked, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::TheMaw => {
            monsters.push(spawn_monster(EnemyId::Maw, monster_hp_rng, slot_counter));
        }
        EncounterId::SphereAndTwoShapes => {
            // Java: getAncientShape() × 2 + SphericGuardian
            let s1 = get_ancient_shape(misc_rng);
            monsters.push(spawn_monster(s1, monster_hp_rng, slot_counter));
            slot_counter += 1;
            let s2 = get_ancient_shape(misc_rng);
            monsters.push(spawn_monster(s2, monster_hp_rng, slot_counter));
            slot_counter += 1;
            monsters.push(spawn_monster(
                EnemyId::SphericGuardian,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::JawWormHorde => {
            // Java: 3 JawWorms with isHorde=true
            for _ in 0..3 {
                let mut jaw_worm = spawn_monster(EnemyId::JawWorm, monster_hp_rng, slot_counter);
                crate::content::monsters::exordium::jaw_worm::initialize_runtime_state(
                    &mut jaw_worm,
                    true,
                );
                monsters.push(jaw_worm);
                slot_counter += 1;
            }
        }
        EncounterId::WrithingMass => {
            monsters.push(spawn_monster(
                EnemyId::WrithingMass,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::GiantHead => {
            monsters.push(spawn_monster(GIANT_HEAD[0], monster_hp_rng, slot_counter));
        }
        EncounterId::TheNemesis => {
            monsters.push(spawn_monster(THE_NEMESIS[0], monster_hp_rng, slot_counter));
        }
        EncounterId::Reptomancer => {
            // Java: SnakeDagger + Reptomancer + SnakeDagger
            let mut left_dagger = spawn_monster(REPTOMANCER[0], monster_hp_rng, slot_counter);
            left_dagger.logical_position =
                crate::content::monsters::beyond::reptomancer::Reptomancer::DAGGER_DRAW_X[1];
            monsters.push(left_dagger);
            slot_counter += 1;
            let mut reptomancer = spawn_monster(REPTOMANCER[1], monster_hp_rng, slot_counter);
            reptomancer.logical_position = 0;
            monsters.push(reptomancer);
            slot_counter += 1;
            let mut right_dagger = spawn_monster(REPTOMANCER[2], monster_hp_rng, slot_counter);
            right_dagger.logical_position =
                crate::content::monsters::beyond::reptomancer::Reptomancer::DAGGER_DRAW_X[0];
            monsters.push(right_dagger);
        }
        EncounterId::AwakenedOne => {
            // Java: 2 Cultists + AwakenedOne
            for enemy in AWAKENED_ONE {
                monsters.push(spawn_monster(enemy, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::TimeEater => {
            monsters.push(spawn_monster(TIME_EATER[0], monster_hp_rng, slot_counter));
        }
        EncounterId::DonuAndDeca => {
            for enemy in DONU_AND_DECA {
                monsters.push(spawn_monster(enemy, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::MysteriousSphere => {
            // Java: getAncientShape() × 2 + OrbWalker
            let s1 = get_ancient_shape(misc_rng);
            monsters.push(spawn_monster(s1, monster_hp_rng, slot_counter));
            slot_counter += 1;
            let s2 = get_ancient_shape(misc_rng);
            monsters.push(spawn_monster(s2, monster_hp_rng, slot_counter));
            slot_counter += 1;
            monsters.push(spawn_monster(
                EnemyId::OrbWalker,
                monster_hp_rng,
                slot_counter,
            ));
        }
        EncounterId::TwoOrbWalkers => {
            for _ in 0..2 {
                monsters.push(spawn_monster(
                    EnemyId::OrbWalker,
                    monster_hp_rng,
                    slot_counter,
                ));
                slot_counter += 1;
            }
        }
        EncounterId::SneckoAndMystics => {
            // Java: Healer + Snecko + Healer
            monsters.push(spawn_monster(EnemyId::Healer, monster_hp_rng, slot_counter));
            slot_counter += 1;
            monsters.push(spawn_monster(EnemyId::Snecko, monster_hp_rng, slot_counter));
            slot_counter += 1;
            monsters.push(spawn_monster(EnemyId::Healer, monster_hp_rng, slot_counter));
        }

        // ==========================================
        // Act 4
        // ==========================================
        EncounterId::ShieldAndSpear => {
            for enemy in SHIELD_AND_SPEAR {
                monsters.push(spawn_monster(enemy, monster_hp_rng, slot_counter));
                slot_counter += 1;
            }
        }
        EncounterId::TheHeart => {
            monsters.push(spawn_monster(THE_HEART[0], monster_hp_rng, slot_counter));
        }
    }

    monsters
}

/// Java: getAncientShape() — random(0..2) → Spiker, Repulsor, or Exploder
fn get_ancient_shape(misc_rng: &mut StsRng) -> EnemyId {
    match misc_rng.random(2) {
        0 => EnemyId::Spiker,
        1 => EnemyId::Repulsor,
        _ => EnemyId::Exploder,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};

    #[test]
    fn described_lineups_match_natural_construction() {
        for encounter in [
            EncounterId::GremlinNob,
            EncounterId::Lagavulin,
            EncounterId::ThreeSentries,
            EncounterId::Slavers,
            EncounterId::BookOfStabbing,
            EncounterId::GremlinLeader,
            EncounterId::MaskedBandits,
            EncounterId::ColosseumSlavers,
            EncounterId::ColosseumNobs,
            EncounterId::TheGuardian,
            EncounterId::Hexaghost,
            EncounterId::SlimeBoss,
            EncounterId::Automaton,
            EncounterId::TheChamp,
            EncounterId::Collector,
            EncounterId::GiantHead,
            EncounterId::TheNemesis,
            EncounterId::Reptomancer,
            EncounterId::AwakenedOne,
            EncounterId::TimeEater,
            EncounterId::DonuAndDeca,
            EncounterId::ShieldAndSpear,
            EncounterId::TheHeart,
        ] {
            let lineup = encounter_lineup(encounter).unwrap();
            for seed in 0..32 {
                let mut actual: Vec<_> =
                    build_encounter(encounter, &mut StsRng::new(seed), &mut StsRng::new(42), 20)
                        .into_iter()
                        .map(|m| m.enemy_id)
                        .collect();
                assert_eq!(actual.len(), lineup.fixed.len() + lineup.random_slots);
                if lineup.random_slots == 0 {
                    assert_eq!(actual, lineup.fixed, "{encounter:?}: initial slot order");
                }
                for enemy in lineup.fixed {
                    let index = actual.iter().position(|id| id == enemy).unwrap();
                    actual.remove(index);
                }
                assert!(actual.iter().all(|id| lineup.random_pool.contains(id)));
            }
        }
        assert!(encounter_lineup(EncounterId::ThreeShapes).is_none());
    }

    #[test]
    fn gremlin_leader_initial_positions_include_wizard_constructor_offset() {
        let mut saw_wizard = false;
        for seed in 0..32 {
            let monsters = build_encounter(
                EncounterId::GremlinLeader,
                &mut StsRng::new(seed),
                &mut StsRng::new(42),
                20,
            );
            for (index, base) in [(0, -366), (1, -170)] {
                let wizard =
                    monsters[index].enemy_id == crate::content::monsters::EnemyId::GremlinWizard;
                saw_wizard |= wizard;
                assert_eq!(
                    monsters[index].logical_position,
                    if wizard { base - 35 } else { base }
                );
            }
            assert_eq!(monsters[2].logical_position, 35);
        }
        assert!(saw_wizard);
    }

    fn monster_summary(monster: &MonsterEntity) -> (EnemyId, i32, Option<i32>) {
        (
            monster.enemy_id,
            monster.current_hp,
            monster.louse.bite_damage,
        )
    }

    fn java_constructor_summary(
        enemy_id: EnemyId,
        hp_rng: &mut StsRng,
        ascension_level: u8,
    ) -> (EnemyId, i32, Option<i32>) {
        let (min, max) = crate::content::monsters::get_hp_range(enemy_id, ascension_level);
        let current_hp = hp_rng.random_range(min, max);
        let louse_bite_damage = match enemy_id {
            EnemyId::LouseNormal | EnemyId::LouseDefensive => Some(if ascension_level >= 2 {
                hp_rng.random_range(6, 8)
            } else {
                hp_rng.random_range(5, 7)
            }),
            _ => None,
        };
        (enemy_id, current_hp, louse_bite_damage)
    }

    fn java_spawn_shapes_sequence(seed: u64, count: usize) -> Vec<EnemyId> {
        let mut rng = StsRng::new(seed);
        let mut pool = vec![
            EnemyId::Repulsor,
            EnemyId::Repulsor,
            EnemyId::Exploder,
            EnemyId::Exploder,
            EnemyId::Spiker,
            EnemyId::Spiker,
        ];
        (0..count)
            .map(|_| {
                let idx = rng.random((pool.len() - 1) as i32) as usize;
                pool.remove(idx)
            })
            .collect()
    }

    fn encounter_enemy_ids(encounter: EncounterId, misc_seed: u64) -> Vec<EnemyId> {
        let mut misc_rng = StsRng::new(misc_seed);
        let mut hp_rng = StsRng::new(99);
        build_encounter(encounter, &mut misc_rng, &mut hp_rng, 0)
            .into_iter()
            .map(|monster| monster.enemy_id)
            .collect()
    }

    #[test]
    fn ancient_shape_encounters_use_java_spawn_shapes_pool_order() {
        let seed = 17;
        assert_eq!(
            encounter_enemy_ids(EncounterId::ThreeShapes, seed),
            java_spawn_shapes_sequence(seed, 3),
            "Java MonsterHelper.spawnShapes starts from Repulsor, Repulsor, Exploder, Exploder, Spiker, Spiker"
        );
        assert_eq!(
            encounter_enemy_ids(EncounterId::FourShapes, seed),
            java_spawn_shapes_sequence(seed, 4),
            "4 Shapes uses the same Java draw-without-replacement pool"
        );
    }

    #[test]
    fn shape_selection_uses_java_random_int_before_each_hp_constructor() {
        for (encounter, ranges, extra_hp_rolls) in [
            (EncounterId::ThreeShapes, vec![5, 4, 3], 0),
            (EncounterId::FourShapes, vec![5, 4, 3, 2], 0),
            (EncounterId::SphereAndTwoShapes, vec![2, 2], 0),
            (EncounterId::MysteriousSphere, vec![2, 2], 2),
        ] {
            let mut misc_rng = StsRng::new(17);
            let mut hp_rng = StsRng::new(99);
            let trace = begin_sts_rng_trace().unwrap();
            build_encounter(encounter, &mut misc_rng, &mut hp_rng, 20);
            let calls = trace.finish().unwrap();
            assert_eq!(calls.len(), ranges.len() * 2 + extra_hp_rolls);
            for (index, expected_range) in ranges.into_iter().enumerate() {
                assert!(matches!(calls[index * 2].call,
                    StsRngCall::RandomInt { range, .. } if range == expected_range));
                assert!(matches!(
                    calls[index * 2 + 1].call,
                    StsRngCall::RandomIntRange { .. }
                ));
            }
        }
    }

    #[test]
    fn spheric_guardian_constructor_does_not_roll_hp_alone_or_with_a_sentry() {
        for (encounter, expected_rolls) in [
            (EncounterId::SphericGuardian, 0),
            (EncounterId::SentryAndSphere, 1),
        ] {
            let mut misc_rng = StsRng::new(1);
            let mut hp_rng = StsRng::new(2);
            let trace = begin_sts_rng_trace().unwrap();
            let monsters = build_encounter(encounter, &mut misc_rng, &mut hp_rng, 20);
            let calls = trace.finish().unwrap();
            assert_eq!(hp_rng.counter, expected_rolls);
            assert_eq!(calls.len(), expected_rolls as usize);
            if expected_rolls == 1 {
                assert!(matches!(
                    calls[0].call,
                    StsRngCall::RandomIntRange {
                        start: 39,
                        end: 45,
                        ..
                    }
                ));
            }
            let sphere = monsters
                .iter()
                .find(|m| m.enemy_id == EnemyId::SphericGuardian)
                .unwrap();
            assert_eq!((sphere.current_hp, sphere.max_hp), (20, 20));
        }
    }

    #[test]
    fn maw_constructor_keeps_super_hp_without_consuming_rng_at_any_ascension() {
        for ascension in [0, 20] {
            let mut misc_rng = StsRng::new(11);
            let mut hp_rng = StsRng::new(29);
            let trace = begin_sts_rng_trace().unwrap();
            let monsters =
                build_encounter(EncounterId::TheMaw, &mut misc_rng, &mut hp_rng, ascension);
            assert!(trace.finish().unwrap().is_empty());
            assert_eq!(hp_rng.counter, 0);
            assert_eq!(monsters.len(), 1);
            assert_eq!(monsters[0].current_hp, 300);
            assert_eq!(monsters[0].max_hp, 300);
        }
    }

    #[test]
    fn transient_constructor_keeps_super_hp_without_consuming_rng() {
        for ascension in [0, 20] {
            let mut misc_rng = StsRng::new(11);
            let mut hp_rng = StsRng::new(29);
            let trace = begin_sts_rng_trace().unwrap();
            let monsters = build_encounter(
                EncounterId::Transient,
                &mut misc_rng,
                &mut hp_rng,
                ascension,
            );
            assert!(trace.finish().unwrap().is_empty());
            assert_eq!(hp_rng.counter, 0);
            assert_eq!(monsters.len(), 1);
            assert_eq!(monsters[0].current_hp, 999);
            assert_eq!(monsters[0].max_hp, 999);
        }
    }

    #[test]
    fn taskmaster_keeps_super_hp_roll_before_final_hp_in_both_encounters() {
        for (ascension, final_range) in
            [(0, (54, 60)), (7, (54, 60)), (8, (57, 64)), (20, (57, 64))]
        {
            for (encounter, enemies) in [
                (
                    EncounterId::Slavers,
                    vec![EnemyId::SlaverBlue, EnemyId::Taskmaster, EnemyId::SlaverRed],
                ),
                (
                    EncounterId::ColosseumNobs,
                    vec![EnemyId::Taskmaster, EnemyId::GremlinNob],
                ),
            ] {
                let mut misc_rng = StsRng::new(11);
                let mut hp_rng = StsRng::new(29);
                let trace = begin_sts_rng_trace().unwrap();
                let monsters = build_encounter(encounter, &mut misc_rng, &mut hp_rng, ascension);
                let calls = trace.finish().unwrap();
                assert_eq!(calls.len(), enemies.len() + 1);
                assert_eq!(hp_rng.counter as usize, calls.len());
                assert_eq!(misc_rng.counter, 0);
                let mut call_index = 0;
                for (monster, enemy) in monsters.iter().zip(enemies) {
                    assert_eq!(monster.enemy_id, enemy);
                    let expected_range = if enemy == EnemyId::Taskmaster {
                        assert!(matches!(
                            calls[call_index].call,
                            StsRngCall::RandomIntRange {
                                start: 54,
                                end: 60,
                                ..
                            }
                        ));
                        call_index += 1;
                        final_range
                    } else {
                        crate::content::monsters::get_hp_range(enemy, ascension)
                    };
                    let StsRngCall::RandomIntRange { start, end, result } = calls[call_index].call
                    else {
                        panic!("monster setHp must use the bounded HP roll");
                    };
                    assert_eq!((start, end), expected_range);
                    assert_eq!((monster.current_hp, monster.max_hp), (result, result));
                    call_index += 1;
                }
                assert_eq!(call_index, calls.len());
            }
        }
    }

    #[test]
    fn reptomancer_samples_super_hp_before_set_hp_between_dagger_constructors() {
        for (ascension, final_range) in [
            (0, (180, 190)),
            (7, (180, 190)),
            (8, (190, 200)),
            (20, (190, 200)),
        ] {
            let mut misc_rng = StsRng::new(11);
            let mut hp_rng = StsRng::new(29);
            let trace = begin_sts_rng_trace().unwrap();
            let monsters = build_encounter(
                EncounterId::Reptomancer,
                &mut misc_rng,
                &mut hp_rng,
                ascension,
            );
            let calls = trace.finish().unwrap();
            let ranges: Vec<_> = calls
                .iter()
                .map(|entry| match &entry.call {
                    StsRngCall::RandomIntRange { start, end, .. } => (*start, *end),
                    other => panic!("unexpected constructor RNG call: {other:?}"),
                })
                .collect();
            assert_eq!(ranges, vec![(20, 25), (180, 190), final_range, (20, 25)]);
            assert_eq!(hp_rng.counter, 4);
            assert_eq!(
                monsters.iter().map(|m| m.enemy_id).collect::<Vec<_>>(),
                vec![
                    EnemyId::SnakeDagger,
                    EnemyId::Reptomancer,
                    EnemyId::SnakeDagger
                ]
            );
            for (monster_index, call_index) in [(0, 0), (1, 2), (2, 3)] {
                let StsRngCall::RandomIntRange { result, .. } = calls[call_index].call else {
                    unreachable!()
                };
                assert_eq!(monsters[monster_index].current_hp, result);
                assert_eq!(monsters[monster_index].max_hp, result);
            }
        }
    }

    #[test]
    fn orb_walker_keeps_both_constructor_hp_rolls_and_uses_the_second_result() {
        for (ascension, final_range) in [
            (0, (90, 96)),
            (6, (90, 96)),
            (7, (92, 102)),
            (20, (92, 102)),
        ] {
            for (encounter, count) in [(EncounterId::OrbWalker, 1), (EncounterId::TwoOrbWalkers, 2)]
            {
                let mut misc_rng = StsRng::new(11);
                let mut hp_rng = StsRng::new(29);
                let trace = begin_sts_rng_trace().unwrap();
                let monsters = build_encounter(encounter, &mut misc_rng, &mut hp_rng, ascension);
                let calls = trace.finish().unwrap();
                assert_eq!(calls.len(), count * 2);
                assert_eq!(hp_rng.counter as usize, count * 2);
                for (index, monster) in monsters.iter().enumerate() {
                    assert!(matches!(
                        calls[index * 2].call,
                        StsRngCall::RandomIntRange {
                            start: 90,
                            end: 96,
                            ..
                        }
                    ));
                    let StsRngCall::RandomIntRange { start, end, result } =
                        calls[index * 2 + 1].call
                    else {
                        panic!("OrbWalker.setHp must make the second bounded HP roll");
                    };
                    assert_eq!((start, end), final_range);
                    assert_eq!((monster.current_hp, monster.max_hp), (result, result));
                }
            }
        }
    }

    #[test]
    fn fixed_hp_act4_encounters_still_consume_monster_hp_rng_like_java_set_hp_int() {
        let mut misc_rng = StsRng::new(1);
        let mut hp_rng = StsRng::new(2);

        let shield_spear =
            build_encounter(EncounterId::ShieldAndSpear, &mut misc_rng, &mut hp_rng, 20);

        assert_eq!(
            hp_rng.counter, 2,
            "Java setHp(int) calls setHp(hp, hp), and Random.random(start,end) still increments the counter"
        );
        assert_eq!(shield_spear.len(), 2);
        assert_eq!(shield_spear[0].current_hp, 125);
        assert_eq!(shield_spear[1].current_hp, 180);
        assert!(
            shield_spear[0].logical_position < 0 && shield_spear[1].logical_position > 0,
            "Shield starts left of the player and Spear starts right; Surrounded BackAttack depends on this Java positioning"
        );

        let mut heart_hp_rng = StsRng::new(3);
        let heart = build_encounter(EncounterId::TheHeart, &mut misc_rng, &mut heart_hp_rng, 20);

        assert_eq!(heart_hp_rng.counter, 1);
        assert_eq!(heart.len(), 1);
        assert_eq!(heart[0].current_hp, 800);
    }

    #[test]
    fn random_hp_monsters_still_consume_monster_hp_rng() {
        let mut misc_rng = StsRng::new(1);
        let mut hp_rng = StsRng::new(2);

        let monsters = build_encounter(EncounterId::JawWorm, &mut misc_rng, &mut hp_rng, 0);

        assert_eq!(monsters.len(), 1);
        assert_eq!(hp_rng.counter, 1);
    }

    #[test]
    fn encounter_pools_use_java_single_bound_draws_from_shrinking_pool() {
        for (encounter, ranges) in [
            (EncounterId::GremlinGang, vec![7, 6, 5, 4]),
            (EncounterId::LotsOfSlimes, vec![4, 3, 2, 1, 0]),
        ] {
            let mut misc_rng = StsRng::new(17);
            misc_rng.bind_trace_owner("floor.misc");
            let mut hp_rng = StsRng::new(23);
            hp_rng.bind_trace_owner("floor.monster_hp");
            let trace = begin_sts_rng_trace().unwrap();

            let monsters = build_encounter(encounter, &mut misc_rng, &mut hp_rng, 20);
            let misc_calls = trace
                .finish()
                .unwrap()
                .into_iter()
                .filter(|event| event.owner == Some("floor.misc"))
                .map(|event| event.call)
                .collect::<Vec<_>>();

            assert_eq!(monsters.len(), ranges.len());
            assert_eq!(misc_calls.len(), ranges.len());
            for (call, expected) in misc_calls.into_iter().zip(ranges) {
                assert!(
                    matches!(call, StsRngCall::RandomInt { range, .. } if range == expected),
                    "{encounter:?}: expected random({expected}), got {call:?}"
                );
            }
        }
    }

    #[test]
    fn exordium_thugs_constructs_discarded_java_candidates_before_selection() {
        for ascension_level in [0, 20] {
            let misc_seed = 17;
            let hp_seed = 23;

            let mut expected_misc = StsRng::new(misc_seed);
            let mut expected_hp = StsRng::new(hp_seed);
            expected_misc.bind_trace_owner("floor.misc");
            expected_hp.bind_trace_owner("floor.monster_hp");
            let expected_trace = begin_sts_rng_trace().unwrap();

            let louse = if expected_misc.random_boolean() {
                EnemyId::LouseNormal
            } else {
                EnemyId::LouseDefensive
            };
            let mut weak_candidates = vec![
                java_constructor_summary(louse, &mut expected_hp, ascension_level),
                java_constructor_summary(EnemyId::SpikeSlimeM, &mut expected_hp, ascension_level),
                java_constructor_summary(EnemyId::AcidSlimeM, &mut expected_hp, ascension_level),
            ];
            let weak_idx = expected_misc.random_range(0, 2) as usize;
            let expected_weak = weak_candidates.remove(weak_idx);

            // MonsterHelper.bottomGetStrongHumanoid constructs Cultist before
            // getSlaver chooses its color. Interleaving matters even when final
            // independent RNG cursors and selected monsters happen to match.
            let cultist =
                java_constructor_summary(EnemyId::Cultist, &mut expected_hp, ascension_level);
            let slaver = if expected_misc.random_boolean() {
                EnemyId::SlaverRed
            } else {
                EnemyId::SlaverBlue
            };
            let mut strong_candidates = vec![
                cultist,
                java_constructor_summary(slaver, &mut expected_hp, ascension_level),
                java_constructor_summary(EnemyId::Looter, &mut expected_hp, ascension_level),
            ];
            let strong_idx = expected_misc.random_range(0, 2) as usize;
            let expected_strong = strong_candidates.remove(strong_idx);
            let expected_calls = expected_trace.finish().unwrap();

            let mut actual_misc = StsRng::new(misc_seed);
            let mut actual_hp = StsRng::new(hp_seed);
            actual_misc.bind_trace_owner("floor.misc");
            actual_hp.bind_trace_owner("floor.monster_hp");
            let actual_trace = begin_sts_rng_trace().unwrap();
            let monsters = build_encounter(
                EncounterId::ExordiumThugs,
                &mut actual_misc,
                &mut actual_hp,
                ascension_level,
            );
            let actual_calls = actual_trace.finish().unwrap();

            assert_eq!(monsters.len(), 2);
            assert_eq!(monster_summary(&monsters[0]), expected_weak);
            assert_eq!(monster_summary(&monsters[1]), expected_strong);
            assert_eq!(actual_misc, expected_misc);
            assert_eq!(
            actual_hp.counter, expected_hp.counter,
            "bottomHumanoid constructs 3 weak candidates and 3 strong candidates before selection"
        );
            assert_eq!(
                actual_calls, expected_calls,
                "constructor calls must follow Java evaluation order"
            );
        }
    }

    #[test]
    fn exordium_wildlife_constructs_discarded_java_candidates_before_selection() {
        let misc_seed = 31;
        let hp_seed = 37;
        let ascension_level = 0;

        let mut expected_misc = StsRng::new(misc_seed);
        let mut expected_hp = StsRng::new(hp_seed);

        let mut strong_candidates = vec![
            java_constructor_summary(EnemyId::FungiBeast, &mut expected_hp, ascension_level),
            java_constructor_summary(EnemyId::JawWorm, &mut expected_hp, ascension_level),
        ];
        let strong_idx = expected_misc.random_range(0, 1) as usize;
        let expected_strong = strong_candidates.remove(strong_idx);

        let louse = if expected_misc.random_boolean() {
            EnemyId::LouseNormal
        } else {
            EnemyId::LouseDefensive
        };
        let mut weak_candidates = vec![
            java_constructor_summary(louse, &mut expected_hp, ascension_level),
            java_constructor_summary(EnemyId::SpikeSlimeM, &mut expected_hp, ascension_level),
            java_constructor_summary(EnemyId::AcidSlimeM, &mut expected_hp, ascension_level),
        ];
        let weak_idx = expected_misc.random_range(0, 2) as usize;
        let expected_weak = weak_candidates.remove(weak_idx);

        let mut actual_misc = StsRng::new(misc_seed);
        let mut actual_hp = StsRng::new(hp_seed);
        let monsters = build_encounter(
            EncounterId::ExordiumWildlife,
            &mut actual_misc,
            &mut actual_hp,
            ascension_level,
        );

        assert_eq!(monsters.len(), 2);
        assert_eq!(monster_summary(&monsters[0]), expected_strong);
        assert_eq!(monster_summary(&monsters[1]), expected_weak);
        assert_eq!(actual_misc, expected_misc);
        assert_eq!(
            actual_hp.counter, expected_hp.counter,
            "bottomWildlife constructs all strong and weak candidates before selecting them"
        );
    }
}
